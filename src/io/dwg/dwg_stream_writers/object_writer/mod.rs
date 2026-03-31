//! DWG Object / Entity Writer (Sprint 4)
//!
//! Writes all DWG object records — table controls, table entries,
//! block headers, entities in each block, and non-graphical objects
//! (dictionaries, layouts, etc.).
//!
//! Ported from ACadSharp `DwgObjectWriter` (partial class across
//! `DwgObjectWriter.cs`, `…Common.cs`, `…Entities.cs`, `…Objects.cs`).
//!
//! ## Record format
//!
//! Each object record in the output stream is:
//! ```text
//! [ModularShort(len)] [merged-stream bytes] [CRC16]
//! ```
//! The merged-stream bytes contain main + text + handle sub-streams
//! interleaved per the DWG spec.

pub mod common;
pub mod entities;
pub mod objects;

use std::collections::VecDeque;

use crate::document::CadDocument;
use crate::entities::{EntityCommon, EntityType};
use crate::io::dwg::dwg_reference_type::DwgReferenceType;
use crate::io::dwg::dwg_stream_writers::DwgMergedWriter;
use crate::io::dwg::dwg_version::DwgVersion;
use crate::tables::BlockRecord;
use crate::types::{BoundingBox3D, DxfVersion, Handle, Vector2};

// ── Helpers ─────────────────────────────────────────────────────────

/// Convert a deduplicated block name back to the DWG binary name.
///
/// In DWG format, all paper-space blocks are stored as `*Paper_Space`
/// and anonymous blocks share names like `*U`, `*D`, etc. (no numeric
/// suffixes).  Our reader adds suffixes (`*Paper_Space0`, `*U1`, …)
/// for deduplication.  This function strips them back for writing.
fn dwg_block_name(name: &str) -> &str {
    // Known multi-word prefixes first
    for prefix in &["*Paper_Space", "*Model_Space"] {
        if name.starts_with(prefix) {
            let rest = &name[prefix.len()..];
            if rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit()) {
                return prefix;
            }
        }
    }
    // Generic anonymous: *<alpha><digits> → *<alpha>
    if name.starts_with('*') && name.len() >= 2 {
        let alpha_end = name[1..]
            .find(|c: char| !c.is_ascii_alphabetic())
            .map(|p| 1 + p)
            .unwrap_or(name.len());
        let rest = &name[alpha_end..];
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return &name[..alpha_end];
        }
    }
    name
}

// ── Public struct ───────────────────────────────────────────────────

/// Writes all DWG object records (entities + table entries + objects)
/// into a contiguous byte stream, tracking handle→offset pairs for
/// the handle section.
pub struct DwgObjectWriter<'a> {
    /// Target DWG version (controls which fields are emitted)
    pub(super) version: DwgVersion,
    /// DXF version (for R2013/R2018 flag checks)
    pub(super) dxf_version: DxfVersion,
    /// Reference to the CAD document being written
    pub(super) document: &'a CadDocument,
    /// Per-object scratch writer (main + text + handle streams)
    pub(super) writer: DwgMergedWriter,
    /// Accumulated output bytes (all object records)
    pub(super) output: Vec<u8>,
    /// Handle → byte-offset map (for handle section)
    pub(super) handle_map: Vec<(u64, u32)>,
    /// Queue of non-graphical objects still to be written
    pub(super) object_queue: VecDeque<Handle>,
    /// Previous entity handle for pre-R2004 entity chain
    pub(super) prev_handle: Option<Handle>,
    /// Next entity handle for pre-R2004 entity chain
    pub(super) next_handle: Option<Handle>,
    /// Next handle value for allocating sub-entity handles (vertices, seqend)
    pub(super) next_alloc_handle: u64,
    /// Computed model space extents for VPort view adjustment and header EXTMIN/EXTMAX
    pub(crate) model_space_extents: Option<BoundingBox3D>,
    /// SAB data entries collected during entity writing (AC1027+).
    /// Each entry is (entity_handle, sab_binary_data).
    pub(super) sab_entries: Vec<(Handle, Vec<u8>)>,
}

impl<'a> DwgObjectWriter<'a> {
    // ── Constructor ─────────────────────────────────────────────────

    /// Create a new object writer for the given document and version.
    pub fn new(document: &'a CadDocument) -> crate::error::Result<Self> {
        let version = DwgVersion::from_dxf_version(document.version)?;
        let dxf_version = document.version;
        let writer = DwgMergedWriter::new(version, dxf_version);
        Ok(Self {
            version,
            dxf_version,
            document,
            writer,
            output: Vec::with_capacity(64 * 1024),
            handle_map: Vec::with_capacity(1024),
            object_queue: VecDeque::new(),
            prev_handle: None,
            next_handle: None,
            next_alloc_handle: document.header.handle_seed,
            model_space_extents: None,
            sab_entries: Vec::new(),
        })
    }

    // ── Main entry point ────────────────────────────────────────────

    /// Write all objects and return `(output_bytes, handle_map, model_space_extents, sab_entries)`.
    ///
    /// For AC1027+, ACIS entities (3DSOLID, REGION, BODY) are written with
    /// `acis_empty=true` in the entity stream; their SAB binary data is
    /// collected into `sab_entries` for writing into the `AcDb:AcDsPrototype_1b`
    /// section.
    pub fn write(mut self) -> (Vec<u8>, Vec<(u64, u32)>, Option<BoundingBox3D>, Vec<(Handle, Vec<u8>)>) {
        // Compute model space extents for VPort view adjustment
        self.model_space_extents = self.compute_model_space_extents();

        // R2004+: 0x0DCA marker at the start
        if self.version.r2004_plus() {
            self.output.extend_from_slice(&0x0DCAi32.to_le_bytes());
        }

        // Enqueue root dictionary for later.
        // If the header handle is NULL (e.g., after a DWG read where the
        // header reader failed to parse handles), scan document.objects to
        // find the root dictionary (a Dictionary with owner == NULL).
        let mut root_dict_handle = self.document.header.named_objects_dict_handle;
        if root_dict_handle.is_null() {
            root_dict_handle = self.find_root_dict_handle();
        }
        if !root_dict_handle.is_null() {
            self.object_queue.push_back(root_dict_handle);
        }

        // ── Table controls ──────────────────────────────────────
        self.write_block_control();
        self.write_table_control(
            self.document.layers.handle(),
            common::OBJ_LAYER_CONTROL,
            &self.document.layers.iter().map(|l| l.handle).collect::<Vec<_>>(),
        );
        self.write_text_style_control();
        self.write_ltype_control();
        self.write_table_control(
            self.document.views.handle(),
            common::OBJ_VIEW_CONTROL,
            &self.document.views.iter().map(|v| v.handle).collect::<Vec<_>>(),
        );
        self.write_table_control(
            self.document.ucss.handle(),
            common::OBJ_UCS_CONTROL,
            &self.document.ucss.iter().map(|u| u.handle).collect::<Vec<_>>(),
        );
        self.write_table_control(
            self.document.vports.handle(),
            common::OBJ_VPORT_CONTROL,
            &self.document.vports.iter().map(|v| v.handle).collect::<Vec<_>>(),
        );
        self.write_table_control(
            self.document.app_ids.handle(),
            common::OBJ_APPID_CONTROL,
            &self.document.app_ids.iter().map(|a| a.handle).collect::<Vec<_>>(),
        );
        self.write_dimstyle_control();

        // R13-R2000 only: VPEntHdr control (viewport entity header table)
        if self.version.r13_15_only() {
            self.write_vpent_hdr_control();
        }

        // ── Table entries ───────────────────────────────────────
        self.write_layer_entries();
        self.write_text_style_entries();
        self.write_ltype_entries();
        self.write_view_entries();
        self.write_ucs_entries();
        self.write_vport_entries();
        self.write_appid_entries();
        self.write_dimstyle_entries();

        // ── Block entities ──────────────────────────────────────
        self.write_block_entities();

        // ── Drain object queue ──────────────────────────────────
        self.write_objects();

        (self.output, self.handle_map, self.model_space_extents, self.sab_entries)
    }

    /// Whether this version stores ACIS data externally (AcDsPrototype_1b section)
    /// rather than inline in the entity stream.
    fn needs_acds_section(&self) -> bool {
        self.dxf_version >= DxfVersion::AC1027
    }

    /// Find the root dictionary handle by scanning document.objects.
    ///
    /// The root dictionary is a Dictionary with `owner == Handle::NULL`.
    /// If multiple candidates exist, prefer the one with the most entries.
    fn find_root_dict_handle(&self) -> Handle {
        let mut best_handle = Handle::NULL;
        let mut best_entry_count = 0usize;

        for (handle, obj) in &self.document.objects {
            if let crate::objects::ObjectType::Dictionary(dict) = obj {
                if dict.owner.is_null() {
                    if dict.entries.len() > best_entry_count
                        || (dict.entries.len() == best_entry_count
                            && handle.value() > best_handle.value())
                    {
                        best_handle = *handle;
                        best_entry_count = dict.entries.len();
                    }
                }
            }
        }

        best_handle
    }

    /// Compute the bounding box of all entities in the *Model_Space block.
    fn compute_model_space_extents(&self) -> Option<BoundingBox3D> {
        let ms_block = self.document.block_records.get("*Model_Space")?;
        let mut extents: Option<BoundingBox3D> = None;
        for eh in &ms_block.entity_handles {
            if let Some(&idx) = self.document.entity_index.get(eh) {
                let bbox = self.document.entities[idx].as_entity().bounding_box();
                extents = Some(match extents {
                    Some(existing) => existing.merge(&bbox),
                    None => bbox,
                });
            }
        }
        extents
    }

    // ── Table control writers ───────────────────────────────────────

    /// Generic table control object: type code, count, soft-owner handles.
    fn write_table_control(
        &mut self,
        table_handle: Handle,
        type_code: i16,
        entry_handles: &[Handle],
    ) {
        // Owner is always 0 for table controls (owned by header)
        self.write_common_non_entity_data(
            type_code,
            table_handle,
            Handle::NULL,
            &[],
            &None,
        );

        // Entry count
        self.writer.write_bit_long(entry_handles.len() as i32);

        // Entry handles (soft ownership)
        for h in entry_handles {
            self.writer
                .write_handle(DwgReferenceType::SoftOwnership, h.value());
        }

        self.register_object(table_handle);
    }

    /// BLOCK_CONTROL — special: excludes *Model_Space and *Paper_Space
    /// from the count, writes them as hard-owner references at the end.
    fn write_block_control(&mut self) {
        let table_handle = self.document.block_records.handle();

        self.write_common_non_entity_data(
            common::OBJ_BLOCK_CONTROL,
            table_handle,
            Handle::NULL,
            &[],
            &None,
        );

        // Gather handles
        let mut regular: Vec<Handle> = Vec::new();
        let mut ms_handle = Handle::NULL;
        let mut ps_handle = Handle::NULL;

        for br in self.document.block_records.iter() {
            if br.name.eq_ignore_ascii_case("*Model_Space") {
                ms_handle = br.handle;
            } else if br.name.eq_ignore_ascii_case("*Paper_Space") {
                ps_handle = br.handle;
            } else {
                regular.push(br.handle);
            }
        }

        // Count excludes model/paper space
        self.writer.write_bit_long(regular.len() as i32);

        for h in &regular {
            self.writer
                .write_handle(DwgReferenceType::SoftOwnership, h.value());
        }

        // *Model_Space, *Paper_Space (hard owner)
        self.writer
            .write_handle(DwgReferenceType::HardOwnership, ms_handle.value());
        self.writer
            .write_handle(DwgReferenceType::HardOwnership, ps_handle.value());

        self.register_object(table_handle);
    }

    /// STYLE_CONTROL
    fn write_text_style_control(&mut self) {
        let handles: Vec<Handle> = self
            .document
            .text_styles
            .iter()
            .map(|s| s.handle)
            .collect();
        self.write_table_control(
            self.document.text_styles.handle(),
            common::OBJ_STYLE_CONTROL,
            &handles,
        );
    }

    /// LTYPE_CONTROL — special: excludes ByLayer/ByBlock from count.
    fn write_ltype_control(&mut self) {
        let table_handle = self.document.line_types.handle();
        self.write_common_non_entity_data(
            common::OBJ_LTYPE_CONTROL,
            table_handle,
            Handle::NULL,
            &[],
            &None,
        );

        let mut regular = Vec::new();
        let mut byblock_handle = Handle::NULL;
        let mut bylayer_handle = Handle::NULL;

        for lt in self.document.line_types.iter() {
            if lt.name.eq_ignore_ascii_case("ByBlock") {
                byblock_handle = lt.handle;
            } else if lt.name.eq_ignore_ascii_case("ByLayer") {
                bylayer_handle = lt.handle;
            } else {
                regular.push(lt.handle);
            }
        }

        self.writer.write_bit_long(regular.len() as i32);
        for h in &regular {
            self.writer
                .write_handle(DwgReferenceType::SoftOwnership, h.value());
        }
        // ByBlock, ByLayer (hard owner)
        self.writer
            .write_handle(DwgReferenceType::HardOwnership, byblock_handle.value());
        self.writer
            .write_handle(DwgReferenceType::HardOwnership, bylayer_handle.value());

        self.register_object(table_handle);
    }

    /// DIMSTYLE_CONTROL — special: has an extra undocumented byte in R2000+.
    fn write_dimstyle_control(&mut self) {
        let table_handle = self.document.dim_styles.handle();
        let handles: Vec<Handle> = self
            .document
            .dim_styles
            .iter()
            .map(|d| d.handle)
            .collect();

        self.write_common_non_entity_data(
            common::OBJ_DIMSTYLE_CONTROL,
            table_handle,
            Handle::NULL,
            &[],
            &None,
        );

        self.writer.write_bit_long(handles.len() as i32);

        // Undocumented byte in R2000+
        if self.version.r2000_plus() {
            self.writer.write_byte(0);
        }

        for h in &handles {
            self.writer
                .write_handle(DwgReferenceType::SoftOwnership, h.value());
        }

        self.register_object(table_handle);
    }

    /// VPENT_HDR_CONTROL — R13-R2000 only.
    /// Empty table control for the viewport entity header table.
    /// The header section references this via hard-ownership handle.
    fn write_vpent_hdr_control(&mut self) {
        let table_handle = self.document.header.vpent_hdr_control_handle;
        if table_handle.is_null() {
            return;
        }

        self.write_table_control(
            table_handle,
            common::OBJ_VPENT_HDR_CONTROL,
            &[], // no entries — always empty
        );
    }

    // ── Table entry writers ─────────────────────────────────────────

    fn write_layer_entries(&mut self) {
        let entries: Vec<_> = self.document.layers.iter().map(|l| l.clone()).collect();
        for layer in &entries {
            self.write_layer(layer);
        }
    }

    fn write_layer(&mut self, layer: &crate::tables::Layer) {
        self.write_common_non_entity_data(
            common::OBJ_LAYER,
            layer.handle,
            self.document.layers.handle(),
            &[],
            &None,
        );

        // Entry name
        self.writer.write_variable_text(&layer.name);

        // Xref-dependant bit
        self.write_xref_dependant_bit_value(layer.flags.xref_dependent);

        if self.version.r2000_plus() {
            let lw_index = layer.line_weight.to_dwg_index() as i16;
            let mut values: i16 = (lw_index & 0x1F) << 5; // lineweight in bits 5..9

            if layer.flags.frozen {
                values |= 0b0001;
            }
            // "off" flag goes in bit 1 (inverted: on → bit clear)
            if layer.flags.off {
                values |= 0b0010;
            }
            // frozen in new VP (bit 2) — always false for now
            // (LayerFlags doesn't expose a separate frozen_in_new_vp flag)
            if false {
                values |= 0b0100;
            }
            if layer.flags.locked {
                values |= 0b1000;
            }
            if layer.is_plottable {
                values |= 0b10000;
            }
            self.writer.write_bit_short(values);
        } else {
            self.writer.write_bit(layer.flags.frozen);
            self.writer.write_bit(layer.flags.off); // off flag (0=on, 1=off, same as R2000+)
            self.writer.write_bit(false); // frozen in new VP
            self.writer.write_bit(layer.flags.locked);
        }

        // Color (CMC)
        self.writer.write_cm_color(&layer.color);

        // External reference block handle
        self.writer
            .write_handle(DwgReferenceType::HardPointer, 0);

        if self.version.r2000_plus() {
            // Plotstyle handle
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
        }

        if self.version.r2007_plus() {
            // Material handle
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
        }

        // Linetype handle — look up by name
        let lt_handle = self
            .document
            .line_types
            .get(&layer.line_type)
            .map(|lt| lt.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, lt_handle.value());

        if self.version.r2013_plus(self.dxf_version) {
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
        }

        self.register_object(layer.handle);
    }

    fn write_text_style_entries(&mut self) {
        let entries: Vec<_> = self
            .document
            .text_styles
            .iter()
            .map(|s| s.clone())
            .collect();
        for style in &entries {
            self.write_text_style(style);
        }
    }

    fn write_text_style(&mut self, style: &crate::tables::TextStyle) {
        self.write_common_non_entity_data(
            common::OBJ_STYLE,
            style.handle,
            self.document.text_styles.handle(),
            &[],
            &None,
        );

        // Entry name
        self.writer.write_variable_text(&style.name);

        // Xref-dependant
        self.write_xref_dependant_bit();

        // Shape file flag
        self.writer.write_bit(false);
        // Vertical flag
        self.writer.write_bit(false);

        // Fixed height
        self.writer.write_bit_double(style.height);
        // Width factor
        self.writer.write_bit_double(style.width_factor);
        // Oblique angle
        self.writer.write_bit_double(style.oblique_angle);
        // Generation (mirror flags)
        self.writer.write_byte(0);
        // Last height (must be > 0; use effective_last_height)
        self.writer.write_bit_double(style.effective_last_height());
        // Font name
        self.writer.write_variable_text(&style.font_file);
        // Big font name
        self.writer.write_variable_text(&style.big_font_file);

        // External reference block handle (hard pointer)
        // Null for non-xref-dependent styles
        self.writer
            .write_handle(DwgReferenceType::HardPointer, 0);

        self.register_object(style.handle);
    }

    fn write_ltype_entries(&mut self) {
        let entries: Vec<_> = self
            .document
            .line_types
            .iter()
            .map(|lt| lt.clone())
            .collect();
        for lt in &entries {
            self.write_line_type(lt);
        }
    }

    fn write_line_type(&mut self, ltype: &crate::tables::LineType) {
        self.write_common_non_entity_data(
            common::OBJ_LTYPE,
            ltype.handle,
            self.document.line_types.handle(),
            &[],
            &None,
        );

        // Entry name
        self.writer.write_variable_text(&ltype.name);
        // Xref
        self.write_xref_dependant_bit();
        // Description
        self.writer.write_variable_text(&ltype.description);
        // Pattern length
        self.writer.write_bit_double(ltype.pattern_length);
        // Alignment
        self.writer.write_byte(b'A');
        // Num dashes
        self.writer.write_byte(ltype.elements.len() as u8);

        for seg in &ltype.elements {
            self.writer.write_bit_double(seg.length);
            self.writer.write_bit_short(0); // shape number
            self.writer.write_raw_double(0.0); // offset x
            self.writer.write_raw_double(0.0); // offset y
            self.writer.write_bit_double(1.0); // scale
            self.writer.write_bit_double(0.0); // rotation
            self.writer.write_bit_short(0); // shape flags
        }

        // R2004 and earlier: 256-byte text area
        if !self.version.r2007_plus() {
            for _ in 0..256 {
                self.writer.write_byte(0);
            }
        }

        // External reference block handle
        self.writer
            .write_handle(DwgReferenceType::HardPointer, 0);

        // Shape file handles for each segment
        for _seg in &ltype.elements {
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
        }

        self.register_object(ltype.handle);
    }

    fn write_view_entries(&mut self) {
        let entries: Vec<_> = self.document.views.iter().map(|v| v.clone()).collect();
        for view in &entries {
            self.write_view(view);
        }
    }

    fn write_view(&mut self, view: &crate::tables::View) {
        self.write_common_non_entity_data(
            common::OBJ_VIEW,
            view.handle,
            self.document.views.handle(),
            &[],
            &None,
        );

        self.writer.write_variable_text(&view.name);
        self.write_xref_dependant_bit();

        self.writer.write_bit_double(view.height);
        self.writer.write_bit_double(view.width);
        self.writer
            .write_2raw_double(crate::types::Vector2 { x: view.center.x, y: view.center.y });
        self.writer.write_3bit_double(view.target);
        self.writer.write_3bit_double(view.direction);
        self.writer.write_bit_double(view.twist_angle);
        self.writer.write_bit_double(view.lens_length);
        self.writer.write_bit_double(view.front_clip);
        self.writer.write_bit_double(view.back_clip);

        // View mode (4 bits)
        self.writer.write_bit(false); // perspective
        self.writer.write_bit(false); // front clipping
        self.writer.write_bit(false); // back clipping
        self.writer.write_bit(false); // front clipping z

        if self.version.r2000_plus() {
            self.writer.write_byte(0); // render mode
        }

        if self.version.r2007_plus() {
            self.writer.write_bit(true);   // use default lights
            self.writer.write_byte(1);     // default lighting
            self.writer.write_bit_double(0.0);
            self.writer.write_bit_double(0.0);
            self.writer.write_cm_color(&crate::types::Color::from_index(250));
        }

        // Paper space flag
        self.writer.write_bit(false);

        if self.version.r2000_plus() {
            // Is UCS associated
            self.writer.write_bit(false);
        }

        // View control handle (soft pointer)
        self.writer
            .write_handle(DwgReferenceType::SoftPointer, self.document.views.handle().value());

        if self.version.r2007_plus() {
            self.writer.write_bit(false); // camera plottable
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, 0);
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
            self.writer
                .write_handle(DwgReferenceType::HardOwnership, 0);
        }

        if self.version.r2007_plus() {
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, 0);
        }

        self.register_object(view.handle);
    }

    fn write_ucs_entries(&mut self) {
        let entries: Vec<_> = self.document.ucss.iter().map(|u| u.clone()).collect();
        for ucs in &entries {
            self.write_ucs(ucs);
        }
    }

    fn write_ucs(&mut self, ucs: &crate::tables::Ucs) {
        self.write_common_non_entity_data(
            common::OBJ_UCS,
            ucs.handle,
            self.document.ucss.handle(),
            &[],
            &None,
        );

        self.writer.write_variable_text(&ucs.name);
        self.write_xref_dependant_bit();

        self.writer.write_3bit_double(ucs.origin);
        self.writer.write_3bit_double(ucs.x_axis);
        self.writer.write_3bit_double(ucs.y_axis);

        if self.version.r2000_plus() {
            self.writer.write_bit_double(0.0);  // elevation
            self.writer.write_bit_short(0);     // ortho view type
            self.writer.write_bit_short(0);     // ortho type
        }

        // UCS control handle
        self.writer
            .write_handle(DwgReferenceType::SoftPointer, self.document.ucss.handle().value());

        if self.version.r2000_plus() {
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
        }

        self.register_object(ucs.handle);
    }

    fn write_vport_entries(&mut self) {
        let mut entries: Vec<_> = self
            .document
            .vports
            .iter()
            .map(|v| v.clone())
            .collect();

        // If model space extents were computed and VPort has default view
        // settings that would miss the entities, apply a "zoom extents"
        // so entities are visible when the file is first opened.
        if let Some(ref ext) = self.model_space_extents {
            let center = ext.center();
            let ext_height = ext.max.y - ext.min.y;
            let ext_width = ext.max.x - ext.min.x;

            for vp in &mut entries {
                // Only adjust the *Active viewport (the main one)
                if vp.name == "*Active" {
                    let ar = if vp.aspect_ratio > 0.0 {
                        vp.aspect_ratio
                    } else {
                        1.0
                    };
                    // Ensure the full extents fit, with 10% margin
                    let vh = (ext_height.max(ext_width / ar)) * 1.1;
                    vp.view_height = if vh > 0.0 { vh } else { 10.0 };
                    // view_center is in DCS, whose origin = view_target.
                    // Keep view_target at its default (origin) so
                    // view_center acts as the WCS center directly.
                    vp.view_center = Vector2::new(center.x, center.y);
                }
            }
        }

        for vp in &entries {
            self.write_vport(vp);
        }
    }

    fn write_vport(&mut self, vport: &crate::tables::VPort) {
        self.write_common_non_entity_data(
            common::OBJ_VPORT,
            vport.handle,
            self.document.vports.handle(),
            &[],
            &None,
        );

        // Common: Entry name TV 2
        self.writer.write_variable_text(&vport.name);
        self.write_xref_dependant_bit();

        // View height BD 40
        self.writer.write_bit_double(vport.view_height);
        // Aspect ratio BD 41 — DWG stores aspect_ratio * view_height
        // (R13 quirk; reader divides by view_height to get actual ratio)
        self.writer
            .write_bit_double(vport.aspect_ratio * vport.view_height);
        // View Center 2RD 12
        self.writer
            .write_2raw_double(crate::types::Vector2 {
                x: vport.view_center.x,
                y: vport.view_center.y,
            });
        // View target 3BD 17
        self.writer.write_3bit_double(vport.view_target);
        // View dir 3BD 16
        self.writer.write_3bit_double(vport.view_direction);
        // View twist BD 51
        self.writer.write_bit_double(0.0);
        // Lens length BD 42
        self.writer.write_bit_double(vport.lens_length);
        // Front clip BD 43
        self.writer.write_bit_double(0.0);
        // Back clip BD 44
        self.writer.write_bit_double(0.0);

        // View mode X 71 — 4 bits: 0123
        self.writer.write_bit(false); // perspective
        self.writer.write_bit(false); // front clipping
        self.writer.write_bit(false); // back clipping
        self.writer.write_bit(false); // front clipping at eye (OPPOSITE of bit 4)

        // R2000+: Render Mode RC 281
        if self.version.r2000_plus() {
            self.writer.write_byte(0);
        }

        // R2007+: lighting
        if self.version.r2007_plus() {
            // Use default lights B 292
            self.writer.write_bit(true);
            // Default lighting type RC 282
            self.writer.write_byte(1);
            // Brightness BD 141
            self.writer.write_bit_double(0.0);
            // Contrast BD 142
            self.writer.write_bit_double(0.0);
            // Ambient Color CMC 63
            self.writer.write_cm_color(&crate::types::Color::from_index(250));
        }

        // Common: Lower left 2RD 10
        self.writer
            .write_2raw_double(crate::types::Vector2 {
                x: vport.lower_left.x,
                y: vport.lower_left.y,
            });
        // Common: Upper right 2RD 11
        self.writer
            .write_2raw_double(crate::types::Vector2 {
                x: vport.upper_right.x,
                y: vport.upper_right.y,
            });

        // UCSFOLLOW B 71
        self.writer.write_bit(false);
        // Circle zoom BS 72
        self.writer.write_bit_short(100);
        // Fast zoom B 73
        self.writer.write_bit(true);
        // UCSICON X 74 — 2 individual bits
        self.writer.write_bit(true); // bit 0: UCS icon display ON
        self.writer.write_bit(true); // bit 1: UCS icon at origin
        // Grid on/off B 76
        self.writer.write_bit(true);
        // Grid spacing 2RD 15
        self.writer
            .write_2raw_double(crate::types::Vector2 {
                x: vport.grid_spacing.x,
                y: vport.grid_spacing.y,
            });
        // Snap on/off B 75
        self.writer.write_bit(false);
        // Snap style B 77
        self.writer.write_bit(false);
        // Snap isopair BS 78
        self.writer.write_bit_short(0);
        // Snap rot BD 50
        self.writer.write_bit_double(0.0);
        // Snap base 2RD 13
        self.writer
            .write_2raw_double(crate::types::Vector2 {
                x: vport.snap_base.x,
                y: vport.snap_base.y,
            });
        // Snap spacing 2RD 14
        self.writer
            .write_2raw_double(crate::types::Vector2 {
                x: vport.snap_spacing.x,
                y: vport.snap_spacing.y,
            });

        // R2000+
        if self.version.r2000_plus() {
            // Unknown B
            self.writer.write_bit(false);
            // UCS per Viewport B 71
            self.writer.write_bit(true);
            // UCS Origin 3BD 110
            self.writer.write_3bit_double(crate::types::Vector3::ZERO);
            // UCS X Axis 3BD 111
            self.writer.write_3bit_double(crate::types::Vector3::UNIT_X);
            // UCS Y Axis 3BD 112
            self.writer.write_3bit_double(crate::types::Vector3::UNIT_Y);
            // UCS Elevation BD 146
            self.writer.write_bit_double(0.0);
            // UCS Orthographic type BS 79
            self.writer.write_bit_short(0);
        }

        // R2007+
        if self.version.r2007_plus() {
            // Grid flags BS 60 — adaptive grid enabled
            self.writer.write_bit_short(2);
            // Grid major BS 61
            self.writer.write_bit_short(5);
        }

        // Common: External reference block handle (hard pointer)
        self.writer.write_handle(DwgReferenceType::HardPointer, 0);

        // R2007+
        if self.version.r2007_plus() {
            // Background handle H 332 soft pointer
            self.writer.write_handle(DwgReferenceType::SoftPointer, 0);
            // Visual Style handle H 348 soft pointer
            self.writer.write_handle(DwgReferenceType::SoftPointer, 0);
            // Sun handle H 361 soft pointer
            self.writer.write_handle(DwgReferenceType::SoftPointer, 0);
        }

        // R2000+
        if self.version.r2000_plus() {
            // Named UCS Handle H 345 hard pointer
            self.writer.write_handle(DwgReferenceType::HardPointer, 0);
            // Base UCS Handle H 346 hard pointer
            self.writer.write_handle(DwgReferenceType::HardPointer, 0);
        }

        self.register_object(vport.handle);
    }

    fn write_appid_entries(&mut self) {
        let entries: Vec<_> = self
            .document
            .app_ids
            .iter()
            .map(|a| a.clone())
            .collect();
        for app in &entries {
            self.write_appid(app);
        }
    }

    fn write_appid(&mut self, app: &crate::tables::AppId) {
        self.write_common_non_entity_data(
            common::OBJ_APPID,
            app.handle,
            self.document.app_ids.handle(),
            &[],
            &None,
        );

        self.writer.write_variable_text(&app.name);
        self.write_xref_dependant_bit();

        // Unknown byte (group 71)
        self.writer.write_byte(0);

        // External reference block handle
        self.writer
            .write_handle(DwgReferenceType::HardPointer, 0);

        self.register_object(app.handle);
    }

    fn write_dimstyle_entries(&mut self) {
        let entries: Vec<_> = self
            .document
            .dim_styles
            .iter()
            .map(|d| d.clone())
            .collect();
        for ds in &entries {
            self.write_dimstyle(ds);
        }
    }

    fn write_dimstyle(&mut self, ds: &crate::tables::DimStyle) {
        self.write_common_non_entity_data(
            common::OBJ_DIMSTYLE,
            ds.handle,
            self.document.dim_styles.handle(),
            &[],
            &None,
        );

        // Common: Entry name TV 2
        self.writer.write_variable_text(&ds.name);
        self.write_xref_dependant_bit();

        // ── R13/R14 Only: DimStyle fields ───────────────────────────
        // These fields are ONLY written for R13/R14 (not R2000+).
        // Field order matches C# ACadSharp writeDimensionStyle() R13_14Only block.
        if self.version.r13_14_only() {
            // DIMTOL B 71
            self.writer.write_bit(ds.dimtol);
            // DIMLIM B 72
            self.writer.write_bit(ds.dimlim);
            // DIMTIH B 73
            self.writer.write_bit(ds.dimtih);
            // DIMTOH B 74
            self.writer.write_bit(ds.dimtoh);
            // DIMSE1 B 75
            self.writer.write_bit(ds.dimse1);
            // DIMSE2 B 76
            self.writer.write_bit(ds.dimse2);
            // DIMALT B 170
            self.writer.write_bit(ds.dimalt);
            // DIMTOFL B 172
            self.writer.write_bit(ds.dimtofl);
            // DIMSAH B 173
            self.writer.write_bit(ds.dimsah);
            // DIMTIX B 174
            self.writer.write_bit(ds.dimtix);
            // DIMSOXD B 175
            self.writer.write_bit(ds.dimsoxd);
            // DIMALTD RC 171
            self.writer.write_byte(ds.dimaltd as u8);
            // DIMZIN RC 78
            self.writer.write_byte(ds.dimzin as u8);
            // DIMSD1 B 281
            self.writer.write_bit(ds.dimsd1);
            // DIMSD2 B 282
            self.writer.write_bit(ds.dimsd2);
            // DIMTOLJ RC 283
            self.writer.write_byte(ds.dimtolj as u8);
            // DIMJUST RC 280
            self.writer.write_byte(ds.dimjust as u8);
            // DIMFIT RC 287
            self.writer.write_byte(3); // default
            // DIMUPT B 288
            self.writer.write_bit(ds.dimupt);
            // DIMTZIN RC 284
            self.writer.write_byte(ds.dimtzin as u8);
            // DIMALTZ RC 285
            self.writer.write_byte(ds.dimaltz as u8);
            // DIMALTTZ RC 286
            self.writer.write_byte(ds.dimalttz as u8);
            // DIMTAD RC 77
            self.writer.write_byte(ds.dimtad as u8);
            // DIMUNIT BS 270
            self.writer.write_bit_short(ds.dimlunit); // R13/R14 uses DIMUNIT (270)
            // DIMAUNIT BS 275
            self.writer.write_bit_short(ds.dimaunit);
            // DIMDEC BS 271
            self.writer.write_bit_short(ds.dimdec);
            // DIMTDEC BS 272
            self.writer.write_bit_short(ds.dimtdec);
            // DIMALTU BS 273
            self.writer.write_bit_short(ds.dimaltu);
            // DIMALTTD BS 274
            self.writer.write_bit_short(ds.dimalttd);
            // DIMSCALE BD 40
            self.writer.write_bit_double(ds.dimscale);
            // DIMASZ BD 41
            self.writer.write_bit_double(ds.dimasz);
            // DIMEXO BD 42
            self.writer.write_bit_double(ds.dimexo);
            // DIMDLI BD 43
            self.writer.write_bit_double(ds.dimdli);
            // DIMEXE BD 44
            self.writer.write_bit_double(ds.dimexe);
            // DIMRND BD 45
            self.writer.write_bit_double(ds.dimrnd);
            // DIMDLE BD 46
            self.writer.write_bit_double(ds.dimdle);
            // DIMTP BD 47
            self.writer.write_bit_double(ds.dimtp);
            // DIMTM BD 48
            self.writer.write_bit_double(ds.dimtm);
            // DIMTXT BD 140
            self.writer.write_bit_double(ds.dimtxt);
            // DIMCEN BD 141
            self.writer.write_bit_double(ds.dimcen);
            // DIMTSZ BD 142
            self.writer.write_bit_double(ds.dimtsz);
            // DIMALTF BD 143
            self.writer.write_bit_double(ds.dimaltf);
            // DIMLFAC BD 144
            self.writer.write_bit_double(ds.dimlfac);
            // DIMTVP BD 145
            self.writer.write_bit_double(ds.dimtvp);
            // DIMTFAC BD 146
            self.writer.write_bit_double(ds.dimtfac);
            // DIMGAP BD 147
            self.writer.write_bit_double(ds.dimgap);
            // DIMPOST T 3
            self.writer.write_variable_text(&ds.dimpost);
            // DIMAPOST T 4
            self.writer.write_variable_text(&ds.dimapost);
            // DIMBLK T 5
            self.writer.write_variable_text("");
            // DIMBLK1 T 6
            self.writer.write_variable_text("");
            // DIMBLK2 T 7
            self.writer.write_variable_text("");
            // DIMCLRD BS 176
            self.writer.write_cm_color(&crate::types::Color::from_index(ds.dimclrd));
            // DIMCLRE BS 177
            self.writer.write_cm_color(&crate::types::Color::from_index(ds.dimclre));
            // DIMCLRT BS 178
            self.writer.write_cm_color(&crate::types::Color::from_index(ds.dimclrt));
        }

        // ── R2000+ DimStyle fields ──────────────────────────────────
        // Field order, data types, and version guards match C# ACadSharp
        // DwgObjectWriter.writeDimensionStyle() exactly.
        if self.version.r2000_plus() {
            // DIMPOST TV 3
            self.writer.write_variable_text(&ds.dimpost);
            // DIMAPOST TV 4
            self.writer.write_variable_text(&ds.dimapost);
            // DIMSCALE BD 40
            self.writer.write_bit_double(ds.dimscale);
            // DIMASZ BD 41
            self.writer.write_bit_double(ds.dimasz);
            // DIMEXO BD 42
            self.writer.write_bit_double(ds.dimexo);
            // DIMDLI BD 43
            self.writer.write_bit_double(ds.dimdli);
            // DIMEXE BD 44
            self.writer.write_bit_double(ds.dimexe);
            // DIMRND BD 45
            self.writer.write_bit_double(ds.dimrnd);
            // DIMDLE BD 46
            self.writer.write_bit_double(ds.dimdle);
            // DIMTP BD 47
            self.writer.write_bit_double(ds.dimtp);
            // DIMTM BD 48
            self.writer.write_bit_double(ds.dimtm);
        }

        // R2007+
        if self.version.r2007_plus() {
            // DIMFXL BD 49
            self.writer.write_bit_double(ds.dimfxl);
            // DIMJOGANG BD 50
            self.writer.write_bit_double(ds.dimjogang);
            // DIMTFILL BS 69
            self.writer.write_bit_short(ds.dimtfill);
            // DIMTFILLCLR CMC 70
            self.writer.write_cm_color(&crate::types::Color::from_index(ds.dimtfillclr));
        }

        // R2000+
        if self.version.r2000_plus() {
            // DIMTOL B 71
            self.writer.write_bit(ds.dimtol);
            // DIMLIM B 72
            self.writer.write_bit(ds.dimlim);
            // DIMTIH B 73
            self.writer.write_bit(ds.dimtih);
            // DIMTOH B 74
            self.writer.write_bit(ds.dimtoh);
            // DIMSE1 B 75
            self.writer.write_bit(ds.dimse1);
            // DIMSE2 B 76
            self.writer.write_bit(ds.dimse2);
            // DIMTAD BS 77
            self.writer.write_bit_short(ds.dimtad);
            // DIMZIN BS 78
            self.writer.write_bit_short(ds.dimzin);
            // DIMAZIN BS 79
            self.writer.write_bit_short(ds.dimazin);
        }

        // R2007+
        if self.version.r2007_plus() {
            // DIMARCSYM BS 90
            self.writer.write_bit_short(ds.dimarcsym);
        }

        // R2000+
        if self.version.r2000_plus() {
            // DIMTXT BD 140
            self.writer.write_bit_double(ds.dimtxt);
            // DIMCEN BD 141
            self.writer.write_bit_double(ds.dimcen);
            // DIMTSZ BD 142
            self.writer.write_bit_double(ds.dimtsz);
            // DIMALTF BD 143
            self.writer.write_bit_double(ds.dimaltf);
            // DIMLFAC BD 144
            self.writer.write_bit_double(ds.dimlfac);
            // DIMTVP BD 145
            self.writer.write_bit_double(ds.dimtvp);
            // DIMTFAC BD 146
            self.writer.write_bit_double(ds.dimtfac);
            // DIMGAP BD 147
            self.writer.write_bit_double(ds.dimgap);
            // DIMALTRND BD 148
            self.writer.write_bit_double(ds.dimaltrnd);
            // DIMALT B 170
            self.writer.write_bit(ds.dimalt);
            // DIMALTD BS 171
            self.writer.write_bit_short(ds.dimaltd);
            // DIMTOFL B 172
            self.writer.write_bit(ds.dimtofl);
            // DIMSAH B 173
            self.writer.write_bit(ds.dimsah);
            // DIMTIX B 174
            self.writer.write_bit(ds.dimtix);
            // DIMSOXD B 175
            self.writer.write_bit(ds.dimsoxd);
            // DIMCLRD BS 176
            self.writer.write_cm_color(&crate::types::Color::from_index(ds.dimclrd));
            // DIMCLRE BS 177
            self.writer.write_cm_color(&crate::types::Color::from_index(ds.dimclre));
            // DIMCLRT BS 178
            self.writer.write_cm_color(&crate::types::Color::from_index(ds.dimclrt));
            // DIMADEC BS 179
            self.writer.write_bit_short(ds.dimadec);
            // DIMDEC BS 271
            self.writer.write_bit_short(ds.dimdec);
            // DIMTDEC BS 272
            self.writer.write_bit_short(ds.dimtdec);
            // DIMALTU BS 273
            self.writer.write_bit_short(ds.dimaltu);
            // DIMALTTD BS 274
            self.writer.write_bit_short(ds.dimalttd);
            // DIMAUNIT BS 275
            self.writer.write_bit_short(ds.dimaunit);
            // DIMFRAC BS 276
            self.writer.write_bit_short(ds.dimfrac);
            // DIMLUNIT BS 277
            self.writer.write_bit_short(ds.dimlunit);
            // DIMDSEP BS 278
            self.writer.write_bit_short(ds.dimdsep);
            // DIMTMOVE BS 279
            self.writer.write_bit_short(ds.dimtmove);
            // DIMJUST BS 280
            self.writer.write_bit_short(ds.dimjust);
            // DIMSD1 B 281
            self.writer.write_bit(ds.dimsd1);
            // DIMSD2 B 282
            self.writer.write_bit(ds.dimsd2);
            // DIMTOLJ BS 283
            self.writer.write_bit_short(ds.dimtolj);
            // DIMTZIN BS 284
            self.writer.write_bit_short(ds.dimtzin);
            // DIMALTZ BS 285
            self.writer.write_bit_short(ds.dimaltz);
            // DIMALTTZ BS 286
            self.writer.write_bit_short(ds.dimalttz);
            // DIMUPT B 288
            self.writer.write_bit(ds.dimupt);
            // DIMFIT BS 287 (hardcoded to 3 per C#)
            self.writer.write_bit_short(3);
        }

        // R2007+
        if self.version.r2007_plus() {
            // DIMFXLON B 290
            self.writer.write_bit(ds.dimfxlon);
        }

        // R2010+
        if self.version.r2010_plus() {
            // DIMTXTDIRECTION B 295
            self.writer.write_bit(ds.dimtxtdirection);
            // DIMALTMZF BD
            self.writer.write_bit_double(0.0);
            // DIMALTMZS T
            self.writer.write_variable_text("");
            // DIMMZF BD
            self.writer.write_bit_double(0.0);
            // DIMMZS T
            self.writer.write_variable_text("");
        }

        // R2000+
        if self.version.r2000_plus() {
            // DIMLWD BS 371
            self.writer.write_bit_short(ds.dimlwd);
            // DIMLWE BS 372
            self.writer.write_bit_short(ds.dimlwe);
        }

        // Common: Unknown B 70
        self.writer.write_bit(false);

        // ── Handle references ───────────────────────────────────────

        // External reference block handle (hard pointer)
        self.writer.write_handle(DwgReferenceType::HardPointer, 0);

        // 340 DIMTXSTY (hard pointer)
        self.writer
            .write_handle(DwgReferenceType::HardPointer, ds.dimtxsty_handle.value());

        // R2000+
        if self.version.r2000_plus() {
            // 341 DIMLDRBLK (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, ds.dimldrblk.value());
            // 342 DIMBLK (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, ds.dimblk.value());
            // 343 DIMBLK1 (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, ds.dimblk1.value());
            // 344 DIMBLK2 (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, ds.dimblk2.value());
        }

        // R2007+
        if self.version.r2007_plus() {
            // 345 dimltype (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, ds.dimltex_handle.value());
            // 346 dimltex1 (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, ds.dimltex1_handle.value());
            // 347 dimltex2 (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, ds.dimltex2_handle.value());
        }

        self.register_object(ds.handle);
    }

    // ── Block entity writing ────────────────────────────────────────

    /// Write block begin/entities/end for every block record.
    fn write_block_entities(&mut self) {
        let block_records: Vec<BlockRecord> = self
            .document
            .block_records
            .iter()
            .map(|br| br.clone())
            .collect();

        for br in &block_records {
            self.write_block_header(br);
            self.write_block_begin(br);

            // Look up entities by handle from the document
            let handles = &br.entity_handles;
            let len = handles.len();
            for (i, eh) in handles.iter().enumerate() {
                if let Some(&idx) = self.document.entity_index.get(eh) {
                    let entity = &self.document.entities[idx];
                    // Set prev/next for entity linking (pre-R2004)
                    self.prev_handle = if i > 0 {
                        Some(handles[i - 1])
                    } else {
                        None
                    };
                    self.next_handle = if i + 1 < len {
                        Some(handles[i + 1])
                    } else {
                        None
                    };

                    self.write_entity(entity);
                }
            }

            self.prev_handle = None;
            self.next_handle = None;

            self.write_block_end(br);
        }
    }

    /// Write a BLOCK_HEADER (block record) object.
    fn write_block_header(&mut self, record: &BlockRecord) {
        let entity_handles = &record.entity_handles;

        self.write_common_non_entity_data(
            common::OBJ_BLOCK_HEADER,
            record.handle,
            self.document.block_records.handle(),
            &[],
            &None,
        );

        // Entry name (DWG uses bare names without numeric suffixes)
        let dwg_name = dwg_block_name(&record.name);
        self.writer.write_variable_text(dwg_name);
        // Xref dependant
        self.write_xref_dependant_bit();

        // Anonymous flag
        self.writer.write_bit(record.flags.anonymous);
        // Has attributes
        self.writer.write_bit(record.flags.has_attributes);
        // Is xref
        self.writer.write_bit(record.flags.is_xref);
        // Is xref overlay
        self.writer.write_bit(record.flags.is_xref_overlay);

        // R2000+: loaded bit
        if self.version.r2000_plus() {
            self.writer.write_bit(false); // is loaded
        }

        // R2004+: owned object count (non-xref)
        if self.version.r2004_plus() && !record.flags.is_xref && !record.flags.is_xref_overlay {
            self.writer.write_bit_long(entity_handles.len() as i32);
        }

        // Base point (from Block entity if found)
        let base_pt = record
            .entity_handles
            .iter()
            .find_map(|eh| {
                if let Some(EntityType::Block(b)) = self.document.entity_index.get(eh).map(|&idx| &self.document.entities[idx]) {
                    Some(b.base_point)
                } else {
                    None
                }
            })
            .unwrap_or(crate::types::Vector3::ZERO);
        self.writer.write_3bit_double(base_pt);

        // Xref path
        self.writer.write_variable_text("");

        // R2000+: insert count bytes + block description
        if self.version.r2000_plus() {
            // Insert count (simplified: 0 inserts)
            self.writer.write_byte(0);

            // Block description
            self.writer.write_variable_text("");

            // Preview data size
            self.writer.write_bit_long(0);
        }

        // R2007+: units, explodable, scaling
        if self.version.r2007_plus() {
            self.writer.write_bit_short(record.units);
            self.writer.write_bit(record.explodable);
            self.writer
                .write_byte(if record.scale_uniformly { 1 } else { 0 });
        }

        // NULL handle (hard pointer)
        self.writer
            .write_handle(DwgReferenceType::HardPointer, 0);

        // BLOCK entity handle (hard owner)
        self.writer.write_handle(
            DwgReferenceType::HardOwnership,
            record.block_entity_handle.value(),
        );

        // R13-R2000: first/last entity handles
        if self.version.r13_15_only() && !record.flags.is_xref && !record.flags.is_xref_overlay {
            let first = entity_handles.first().copied().unwrap_or(Handle::NULL);
            let last = entity_handles.last().copied().unwrap_or(Handle::NULL);
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, first.value());
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, last.value());
        }

        // R2004+: entity handles (hard owner)
        if self.version.r2004_plus() {
            for h in entity_handles {
                self.writer
                    .write_handle(DwgReferenceType::HardOwnership, h.value());
            }
        }

        // ENDBLK handle (hard owner)
        self.writer.write_handle(
            DwgReferenceType::HardOwnership,
            record.block_end_handle.value(),
        );

        // R2000+: layout handle
        if self.version.r2000_plus() {
            self.writer
                .write_handle(DwgReferenceType::HardPointer, record.layout.value());
        }

        self.register_object(record.handle);
    }

    /// Write BLOCK entity (block begin).
    fn write_block_begin(&mut self, record: &BlockRecord) {
        let block = record
            .entity_handles
            .iter()
            .find_map(|eh| {
                if let Some(EntityType::Block(b)) = self.document.entity_index.get(eh).map(|&idx| &self.document.entities[idx]) {
                    Some(b.clone())
                } else {
                    None
                }
            });

        let (handle, name) = if let Some(ref b) = block {
            (b.common.handle, b.name.as_str())
        } else {
            (record.block_entity_handle, record.name.as_str())
        };

        let common = block
            .as_ref()
            .map(|b| &b.common)
            .cloned()
            .unwrap_or_else(|| EntityCommon {
                handle,
                owner_handle: record.handle,
                ..Default::default()
            });

        self.write_common_entity_data(
            common::OBJ_BLOCK,
            common.handle,
            common.owner_handle,
            &common.layer,
            &common.color,
            &common.line_weight,
            &common.transparency,
            common.invisible,
            common.linetype_scale,
            &common.linetype,
            &common.extended_data,
            &common.reactors,
            &common.xdictionary_handle,
        );

        self.writer.write_variable_text(dwg_block_name(name));

        self.register_object(common.handle);
    }

    /// Write ENDBLK entity (block end).
    fn write_block_end(&mut self, record: &BlockRecord) {
        let block_end = record
            .entity_handles
            .iter()
            .find_map(|eh| {
                if let Some(EntityType::BlockEnd(be)) = self.document.entity_index.get(eh).map(|&idx| &self.document.entities[idx]) {
                    Some(be.clone())
                } else {
                    None
                }
            });

        let common = block_end
            .map(|be| be.common)
            .unwrap_or_else(|| EntityCommon {
                handle: record.block_end_handle,
                owner_handle: record.handle,
                ..Default::default()
            });

        self.write_common_entity_data(
            common::OBJ_ENDBLK,
            common.handle,
            common.owner_handle,
            &common.layer,
            &common.color,
            &common.line_weight,
            &common.transparency,
            common.invisible,
            common.linetype_scale,
            &common.linetype,
            &common.extended_data,
            &common.reactors,
            &common.xdictionary_handle,
        );

        self.register_object(common.handle);
    }

    // ── Object queue draining ───────────────────────────────────────

    /// Drain the object queue, writing each non-graphical object.
    fn write_objects(&mut self) {
        while let Some(handle) = self.object_queue.pop_front() {
            if let Some(obj) = self.document.objects.get(&handle) {
                let obj = obj.clone();
                self.write_object(&obj);
            }
        }
    }

    // ── Access helpers ──────────────────────────────────────────────

    /// Get the output bytes.
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// Get the handle map.
    pub fn handle_map(&self) -> &[(u64, u32)] {
        &self.handle_map
    }
}

// ── Tests ──────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_writer_creates_for_default_document() {
        let doc = CadDocument::new();
        let writer = DwgObjectWriter::new(&doc);
        assert!(writer.is_ok());
    }

    #[test]
    fn object_writer_writes_basic_document() {
        let doc = CadDocument::new();
        let writer = DwgObjectWriter::new(&doc).unwrap();
        let (output, handle_map, _, _) = writer.write();
        // Should have produced some output (at least the 0x0DCA marker)
        assert!(!output.is_empty());
        // Should have recorded some handles (table controls + entries)
        assert!(!handle_map.is_empty());
    }

    #[test]
    fn object_writer_encodes_dca_marker() {
        let doc = CadDocument::new();
        let writer = DwgObjectWriter::new(&doc).unwrap();
        let (output, _, _, _) = writer.write();
        // First 4 bytes should be 0x0DCA as little-endian i32
        if output.len() >= 4 {
            let marker = i32::from_le_bytes([output[0], output[1], output[2], output[3]]);
            assert_eq!(marker, 0x0DCA);
        }
    }
}
