//! Non-graphical object serialization for DWG records.
//!
//! Handles dictionaries, layouts, plot-settings, XRecords, groups,
//! mline styles, image definitions, etc.
//!
//! Each writer:
//! 1. Calls `write_common_non_entity_data()` (type + handle + reactors)
//! 2. Writes type-specific fields
//! 3. Calls `register_object()` (CRC, output, handle map)
//!
//! Ported from ACadSharp `DwgObjectWriter.Objects.cs`.

use crate::io::dwg::dwg_reference_type::DwgReferenceType;
use crate::objects::*;
use crate::types::{DxfVersion, Handle};

use super::common;
use super::DwgObjectWriter;

/// Flatten a [`Matrix4`](crate::types::Matrix4) into 12 doubles holding its 3×4
/// part in row-major order (3 rows of 4); the bottom row is dropped. DWG stores
/// the spatial-filter transforms row-major.
fn matrix_to_row_major(m: &crate::types::Matrix4) -> [f64; 12] {
    let mut out = [0.0; 12];
    let mut i = 0;
    for row in 0..3 {
        for col in 0..4 {
            out[i] = m.m[row][col];
            i += 1;
        }
    }
    out
}

impl<'a> DwgObjectWriter<'a> {
    // ── Object dispatch ─────────────────────────────────────────────

    /// Write a single non-graphical object record.
    pub(super) fn write_object(&mut self, obj: &ObjectType) {
        match obj {
            ObjectType::Dictionary(d) => self.write_dictionary(d),
            ObjectType::Layout(l) => self.write_layout(l),
            ObjectType::XRecord(x) => self.write_xrecord(x),
            ObjectType::Group(g) => self.write_group(g),
            ObjectType::MLineStyle(m) => self.write_mlinestyle(m),
            ObjectType::MultiLeaderStyle(m) => self.write_multileader_style(m),
            ObjectType::ImageDefinition(d) => self.write_image_definition(d),
            ObjectType::ImageDefinitionReactor(r) => self.write_image_definition_reactor(r),
            ObjectType::PlotSettings(p) => self.write_plot_settings_obj(p),
            ObjectType::Scale(s) => self.write_scale(s),
            ObjectType::SortEntitiesTable(s) => self.write_sort_entities_table(s),
            ObjectType::DictionaryVariable(d) => self.write_dictionary_variable(d),
            ObjectType::RasterVariables(r) => self.write_raster_variables(r),
            ObjectType::DictionaryWithDefault(d) => self.write_dictionary_with_default(d),
            ObjectType::PlaceHolder(p) => self.write_placeholder(p),
            ObjectType::BookColor(b) => self.write_book_color(b),
            ObjectType::WipeoutVariables(w) => self.write_wipeout_variables(w),
            ObjectType::SpatialFilter(s) => self.write_spatial_filter(s),
            // Stub / unsupported objects — skip
            ObjectType::GeoData(_)
            | ObjectType::VisualStyle(_)
            | ObjectType::Material(_)
            | ObjectType::TableStyle(_) => {}
            ObjectType::Unknown { handle, raw_dwg_data, raw_dwg_handle_bits, .. } => {
                if let Some(ref raw) = raw_dwg_data {
                    self.register_raw_object(*handle, raw, *raw_dwg_handle_bits);
                }
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────

    /// Returns `true` when the object at `handle` will actually be
    /// serialized by `write_object`.  Entries pointing to un-writable
    /// objects (VisualStyle, Material, TableStyle, etc.) must be
    /// excluded from dictionary records so the DWG doesn't contain
    /// dangling handle references.
    fn is_writable_object(&self, handle: &Handle) -> bool {
        match self.document.objects.get(handle) {
            None => false,
            Some(obj) => match obj {
                ObjectType::GeoData(_)
                | ObjectType::VisualStyle(_)
                | ObjectType::Material(_)
                | ObjectType::TableStyle(_) => false,
                ObjectType::Unknown { type_name, raw_dwg_data, .. } => {
                    // Exclude types that would also be excluded if parsed into proper variants
                    if type_name.starts_with("DWG_OBJ_106") // TABLESTYLE
                        || type_name.starts_with("DWG_OBJ_105") // TABLECONTENT
                    {
                        return false;
                    }
                    raw_dwg_data.is_some()
                }
                _ => true,
            },
        }
    }

    // ── Dictionary ──────────────────────────────────────────────────

    fn write_dictionary(&mut self, dict: &Dictionary) {
        // For pre-R2000, filter out R2000+-only dictionary entries
        // (PLOTSTYLENAME, LAYOUT, PLOTSETTINGS, MATERIAL, COLOR, VISUALSTYLE)
        let entries: Vec<&(String, Handle)> = if self.version.r2000_plus() {
            dict.entries.iter()
                .filter(|(_, h)| !h.is_null() && self.is_writable_object(h))
                .collect()
        } else {
            dict.entries.iter().filter(|(name, h)| {
                !matches!(name.as_str(),
                    "ACAD_PLOTSTYLENAME" | "ACAD_LAYOUT" | "ACAD_PLOTSETTINGS" |
                    "ACAD_MATERIAL" | "ACAD_COLOR" | "ACAD_VISUALSTYLE"
                ) && !h.is_null() && self.is_writable_object(h)
            }).collect()
        };

        self.write_common_non_entity_data(
            common::OBJ_DICTIONARY,
            dict.handle,
            dict.owner,
            &dict.reactors,
            &dict.xdictionary_handle,
        );

        // Number of entries (BL)
        self.writer.write_bit_long(entries.len() as i32);

        // R14 Only: Unknown byte (always 0)
        if self.dxf_version == DxfVersion::AC1014 {
            self.writer.write_byte(0);
        }

        // R2000+: Cloning flag (BS 281) + Hard-owner flag (RC)
        if self.version.r2000_plus() {
            self.writer.write_bit_short(dict.duplicate_cloning as i16);
            self.writer.write_byte(if dict.hard_owner { 1 } else { 0 });
        }

        // Entry names + handles
        for (name, handle) in &entries {
            self.writer.write_variable_text(name);
            let ref_type = if dict.hard_owner {
                DwgReferenceType::HardOwnership
            } else {
                DwgReferenceType::SoftOwnership
            };
            self.writer.write_handle(ref_type, handle.value());

            // Enqueue referenced objects
            if !handle.is_null() {
                self.object_queue.push_back(*handle);
            }
        }

        self.register_object(dict.handle);
    }

    // ── Dictionary with default ─────────────────────────────────────

    fn write_dictionary_with_default(&mut self, dict: &DictionaryWithDefault) {
        // Pre-R2000: ACDBDICTIONARYWDFLT class doesn't exist, so fall back
        // to writing as a regular Dictionary (skip the default_handle field).
        if !self.version.r2000_plus() {
            let plain = Dictionary {
                handle: dict.handle,
                owner: dict.owner,
                hard_owner: dict.hard_owner,
                duplicate_cloning: dict.duplicate_cloning,
                entries: dict.entries.clone(),
                reactors: vec![],
                xdictionary_handle: None,
            };
            self.write_dictionary(&plain);
            return;
        }

        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("ACDBDICTIONARYWDFLT", common::OBJ_DICTIONARYWDFLT);

        self.write_common_non_entity_data(
            type_code,
            dict.handle,
            dict.owner,
            &[],
            &None,
        );

        // Filter out entries referencing un-writable objects
        let entries: Vec<&(String, Handle)> = dict.entries.iter()
            .filter(|(_, h)| h.is_null() || self.is_writable_object(h))
            .collect();

        // Same as dictionary
        self.writer.write_bit_long(entries.len() as i32);

        // R2000+: Cloning flag (BS) + Hard-owner flag (RC)
        self.writer.write_bit_short(dict.duplicate_cloning as i16);
        self.writer.write_byte(if dict.hard_owner { 1 } else { 0 });

        for (name, handle) in &entries {
            self.writer.write_variable_text(name);
            let ref_type = if dict.hard_owner {
                DwgReferenceType::HardOwnership
            } else {
                DwgReferenceType::SoftOwnership
            };
            self.writer.write_handle(ref_type, handle.value());

            if !handle.is_null() {
                self.object_queue.push_back(*handle);
            }
        }

        // Default entry handle
        self.writer
            .write_handle(DwgReferenceType::HardPointer, dict.default_handle.value());

        self.register_object(dict.handle);
    }

    // ── Dictionary Variable ─────────────────────────────────────────

    fn write_dictionary_variable(&mut self, dv: &DictionaryVariable) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("DICTIONARYVAR", common::OBJ_DICTIONARYVAR);
        self.write_common_non_entity_data(
            type_code,
            dv.handle,
            dv.owner_handle,
            &[],
            &None,
        );

        self.writer.write_byte(0); // object schema number
        self.writer.write_variable_text(&dv.value);

        self.register_object(dv.handle);
    }

    // ── Layout (extends PlotSettings) ───────────────────────────────
    //
    // Field order must match C# DwgObjectWriter.Objects.cs writeLayout()
    // exactly. Layout extends PlotSettings, so PlotSettings fields come
    // first, then Layout-specific fields.

    fn write_layout(&mut self, layout: &Layout) {
        // For pre-R2004, LAYOUT is an UNLISTED type — must use the DXF
        // class number instead of the fixed type code 82.
        let type_code = if self.version.r2004_pre() {
            self.document
                .classes
                .get_by_name("LAYOUT")
                .map(|c| c.class_number)
                .unwrap_or(common::OBJ_LAYOUT)
        } else {
            common::OBJ_LAYOUT
        };

        self.write_common_non_entity_data(
            type_code,
            layout.handle,
            layout.owner,
            &layout.reactors,
            &layout.xdictionary_handle,
        );

        // ── PlotSettings preamble ──
        // ModelType flag (bit 0x400) must be set for model space layouts
        let plot_flags: i16 = if layout.name == "Model" { 0x400 } else { 0 };
        self.write_plot_settings_data(plot_flags, layout);

        // ── Layout-specific data ──
        // Layout name (TV)
        self.writer.write_variable_text(&layout.name);
        // Tab order (BL 71)
        self.writer.write_bit_long(layout.tab_order as i32);
        // Layout flags (BS 70)
        self.writer.write_bit_short(layout.flags);

        // UCS origin (3BD 13) — layout UCS origin
        self.writer
            .write_3bit_double(crate::types::Vector3::ZERO);

        // Min limits (2RD 10)
        self.writer.write_raw_double(layout.min_limits.0);
        self.writer.write_raw_double(layout.min_limits.1);
        // Max limits (2RD 11)
        self.writer.write_raw_double(layout.max_limits.0);
        self.writer.write_raw_double(layout.max_limits.1);

        // Insertion base (3BD 12)
        self.writer
            .write_3bit_double(crate::types::Vector3::new(
                layout.insertion_base.0,
                layout.insertion_base.1,
                layout.insertion_base.2,
            ));

        // X axis direction (3BD)
        self.writer
            .write_3bit_double(crate::types::Vector3::UNIT_X);
        // Y axis direction (3BD)
        self.writer
            .write_3bit_double(crate::types::Vector3::UNIT_Y);

        // Elevation (BD)
        self.writer.write_bit_double(0.0);

        // UCS orthographic type (BS)
        self.writer.write_bit_short(0);

        // Min extents (3BD)
        self.writer
            .write_3bit_double(crate::types::Vector3::new(
                layout.min_extents.0,
                layout.min_extents.1,
                layout.min_extents.2,
            ));
        // Max extents (3BD)
        self.writer
            .write_3bit_double(crate::types::Vector3::new(
                layout.max_extents.0,
                layout.max_extents.1,
                layout.max_extents.2,
            ));

        // R2004+: Viewport count (BL)
        if self.version.r2004_plus() {
            self.writer.write_bit_long(0); // no viewports
        }

        // ── Handle references ──
        // 330 Associated block record (soft pointer)
        self.writer
            .write_handle(DwgReferenceType::SoftPointer, layout.block_record.value());

        // 331 Last active viewport (soft pointer)
        self.writer
            .write_handle(DwgReferenceType::SoftPointer, layout.viewport.value());

        // UCS handles — ortho type is 0 (None), so:
        // 346 base UCS handle (hard pointer) — null
        self.writer
            .write_handle(DwgReferenceType::HardPointer, 0);
        // 345 named UCS handle (hard pointer) — null
        self.writer
            .write_handle(DwgReferenceType::HardPointer, 0);

        // R2004+: Viewport handles (repeated count times — 0 for us)
        // (nothing to write since viewport count is 0)

        self.register_object(layout.handle);
    }

    /// Write the PlotSettings portion of a Layout record.
    ///
    /// Field order must match C# DwgObjectWriter.Objects.cs writePlotSettings()
    /// exactly. Uses simplified/default values.
    fn write_plot_settings_data(&mut self, plot_flags: i16, layout: &crate::objects::Layout) {
        // Fall back to A4 landscape when the layout carries no paper size
        // (e.g. freshly created layouts that never read a PlotSettings block).
        let (paper_width, paper_height) =
            if layout.paper_width > 0.0 && layout.paper_height > 0.0 {
                (layout.paper_width, layout.paper_height)
            } else {
                (297.0, 210.0)
            };
        // Page setup name (TV 1)
        self.writer.write_variable_text(&layout.plot_page_name);
        // Printer / Config (TV 2)
        self.writer.write_variable_text(&layout.plot_printer_name);
        // Plot layout flags (BS 70)
        self.writer.write_bit_short(plot_flags);

        // Margins (BD: left, bottom, right, top)
        self.writer.write_bit_double(layout.plot_margin_left);
        self.writer.write_bit_double(layout.plot_margin_bottom);
        self.writer.write_bit_double(layout.plot_margin_right);
        self.writer.write_bit_double(layout.plot_margin_top);

        // Paper width (BD 44), height (BD 45)
        self.writer.write_bit_double(paper_width);
        self.writer.write_bit_double(paper_height);

        // Paper size (TV 4)
        self.writer.write_variable_text(&layout.paper_size);

        // Plot origin (2BD 46,47)
        self.writer.write_bit_double(layout.plot_origin_x);
        self.writer.write_bit_double(layout.plot_origin_y);

        // Paper units (BS 72), Plot rotation (BS 73), Plot type (BS 74)
        self.writer.write_bit_short(layout.plot_paper_units);
        self.writer.write_bit_short(layout.plot_rotation);
        self.writer.write_bit_short(layout.plot_type);

        // Plot window (2BD min, 2BD max)
        self.writer.write_bit_double(layout.plot_window_min_x);
        self.writer.write_bit_double(layout.plot_window_min_y);
        self.writer.write_bit_double(layout.plot_window_max_x);
        self.writer.write_bit_double(layout.plot_window_max_y);

        // R13-R2000 only: Plot view name (TV 6)
        if self.version.r13_15_only() {
            self.writer.write_variable_text(&layout.plot_view_name);
        }

        // Real world units / numerator (BD 142)
        let num = if layout.plot_scale_numerator != 0.0 { layout.plot_scale_numerator } else { 1.0 };
        let den = if layout.plot_scale_denominator != 0.0 { layout.plot_scale_denominator } else { 1.0 };
        self.writer.write_bit_double(num);
        // Drawing units / denominator (BD 143)
        self.writer.write_bit_double(den);

        // Current style sheet (TV 7)
        self.writer.write_variable_text(&layout.plot_style_sheet);

        // Scale type (BS 75)
        self.writer.write_bit_short(layout.plot_scale_type);

        // Scale factor (BD 147) — standard scale value
        let factor = if layout.plot_scale_factor != 0.0 { layout.plot_scale_factor } else { 1.0 };
        self.writer.write_bit_double(factor);

        // Paper image origin (2BD 148,149)
        self.writer.write_bit_double(layout.paper_image_origin_x);
        self.writer.write_bit_double(layout.paper_image_origin_y);

        // R2004+: shade plot fields
        if self.version.r2004_plus() {
            self.writer.write_bit_short(layout.shade_plot_mode);
            self.writer.write_bit_short(layout.shade_plot_resolution);
            let dpi = if layout.shade_plot_dpi != 0 { layout.shade_plot_dpi } else { 300 };
            self.writer.write_bit_short(dpi);

            // Plot view handle (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
        }

        // R2007+: visual style handle
        if self.version.r2007_plus() {
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, 0);
        }
    }

    /// Write a standalone PlotSettings object.
    fn write_plot_settings_obj(&mut self, ps: &PlotSettings) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("PLOTSETTINGS", common::OBJ_PLOTSETTINGS);

        self.write_common_non_entity_data(
            type_code,
            ps.handle,
            ps.owner,
            &[],
            &None,
        );

        // Field order must match C# writePlotSettings() exactly
        // Page setup name (TV 1)
        self.writer.write_variable_text(&ps.page_name);
        // Printer / Config (TV 2)
        self.writer.write_variable_text(&ps.printer_name);
        // Plot layout flags (BS 70)
        self.writer.write_bit_short(0);

        // Margins (BD: left, bottom, right, top)
        self.writer.write_bit_double(ps.margins.left);
        self.writer.write_bit_double(ps.margins.bottom);
        self.writer.write_bit_double(ps.margins.right);
        self.writer.write_bit_double(ps.margins.top);

        // Paper width (BD 44), height (BD 45)
        self.writer.write_bit_double(ps.paper_width);
        self.writer.write_bit_double(ps.paper_height);

        // Paper size (TV 4)
        self.writer.write_variable_text(&ps.paper_size);

        // Plot origin (2BD 46,47)
        self.writer.write_bit_double(ps.origin_x);
        self.writer.write_bit_double(ps.origin_y);

        // Paper units (BS 72), Plot rotation (BS 73), Plot type (BS 74)
        self.writer.write_bit_short(ps.paper_units as i16);
        self.writer.write_bit_short(ps.rotation as i16);
        self.writer.write_bit_short(ps.plot_type as i16);

        // Plot window (2BD min, 2BD max)
        self.writer.write_bit_double(ps.plot_window.lower_left_x);
        self.writer.write_bit_double(ps.plot_window.lower_left_y);
        self.writer.write_bit_double(ps.plot_window.upper_right_x);
        self.writer.write_bit_double(ps.plot_window.upper_right_y);

        // R13-R2000 only: Plot view name (TV 6)
        if self.version.r13_15_only() {
            self.writer.write_variable_text(&ps.plot_view_name);
        }

        // Real world units / numerator (BD 142)
        self.writer.write_bit_double(ps.scale_numerator);
        // Drawing units / denominator (BD 143)
        self.writer.write_bit_double(ps.scale_denominator);

        // Current style sheet (TV 7)
        self.writer.write_variable_text(&ps.current_style_sheet);

        // Scale type (BS 75)
        self.writer.write_bit_short(ps.scale_type as i16);

        // Scale factor (BD 147)
        self.writer.write_bit_double(1.0);

        // Paper image origin (2BD 148,149)
        self.writer.write_bit_double(0.0);
        self.writer.write_bit_double(0.0);

        // R2004+: shade plot fields
        if self.version.r2004_plus() {
            self.writer.write_bit_short(ps.shade_plot_mode as i16);
            self.writer
                .write_bit_short(ps.shade_plot_resolution as i16);
            self.writer.write_bit_short(ps.shade_plot_dpi);

            // Plot view handle (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
        }

        // R2007+: visual style handle
        if self.version.r2007_plus() {
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, 0);
        }

        self.register_object(ps.handle);
    }

    // ── Group ───────────────────────────────────────────────────────

    fn write_group(&mut self, group: &Group) {
        self.write_common_non_entity_data(
            common::OBJ_GROUP,
            group.handle,
            group.owner,
            &[],
            &None,
        );

        self.writer.write_variable_text(&group.description);
        self.writer.write_bit_short(1); // unnamed flag (0=named)
        self.writer
            .write_bit_short(if group.selectable { 1 } else { 0 });

        // Entity handles
        self.writer
            .write_bit_long(group.entities.len() as i32);
        for h in &group.entities {
            self.writer
                .write_handle(DwgReferenceType::HardPointer, h.value());
        }

        self.register_object(group.handle);
    }

    // ── MLineStyle ──────────────────────────────────────────────────

    fn write_mlinestyle(&mut self, style: &MLineStyle) {
        self.write_common_non_entity_data(
            common::OBJ_MLINESTYLE,
            style.handle,
            style.owner,
            &[],
            &None,
        );

        self.writer.write_variable_text(&style.name);
        self.writer.write_variable_text(&style.description);

        // Flags — DWG binary format swaps some pairs vs the DXF enum:
        //   DWG bit 1 = DisplayJoints, bit 2 = FillOn
        //   (DXF enum: FillOn=1, DisplayJoints=2)
        //   DWG: StartRound=0x20, StartInner=0x40
        //   (DXF: StartInner=0x20, StartRound=0x40)
        //   DWG: EndRound=0x200, EndInner=0x400
        //   (DXF: EndInner=0x200, EndRound=0x400)
        let mut flags: i16 = 0;
        if style.flags.display_joints { flags |= 1; }
        if style.flags.fill_on { flags |= 2; }
        if style.flags.start_square_cap { flags |= 16; }
        if style.flags.start_round_cap { flags |= 0x20; }
        if style.flags.start_inner_arcs_cap { flags |= 0x40; }
        if style.flags.end_square_cap { flags |= 0x100; }
        if style.flags.end_round_cap { flags |= 0x200; }
        if style.flags.end_inner_arcs_cap { flags |= 0x400; }
        self.writer.write_bit_short(flags);

        self.writer.write_cm_color(&style.fill_color);
        self.writer.write_bit_double(style.start_angle);
        self.writer.write_bit_double(style.end_angle);

        // Elements
        self.writer
            .write_byte(style.elements.len() as u8);
        for elem in &style.elements {
            self.writer.write_bit_double(elem.offset);
            self.writer.write_cm_color(&elem.color);

            if self.version.r2018_plus(self.dxf_version) {
                // R2018+: Line type handle (hard pointer)
                let lt_handle = self
                    .document
                    .line_types
                    .get(&elem.linetype)
                    .map(|lt| lt.handle)
                    .unwrap_or(Handle::NULL);
                self.writer
                    .write_handle(DwgReferenceType::HardPointer, lt_handle.value());
            } else {
                // Before R2018: Ltindex BS (linetype index, 0 = BYLAYER)
                self.writer.write_bit_short(0);
            }
        }

        self.register_object(style.handle);
    }

    // ── MultiLeaderStyle ────────────────────────────────────────────

    fn write_multileader_style(&mut self, style: &MultiLeaderStyle) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("MLEADERSTYLE", common::OBJ_MLEADERSTYLE);
        self.write_common_non_entity_data(
            type_code,
            style.handle,
            style.owner_handle,
            &[],
            &None,
        );

        // R2010+: Version (BS, expected 2)
        if self.version.r2010_plus() {
            self.writer.write_bit_short(2);
        }

        // Content type
        self.writer
            .write_bit_short(style.content_type as i16);
        // Draw order
        self.writer
            .write_bit_short(style.multileader_draw_order as i16);
        self.writer
            .write_bit_short(style.leader_draw_order as i16);

        // Max leader points
        self.writer
            .write_bit_long(style.max_leader_points);
        // Segment angles
        self.writer
            .write_bit_double(style.first_segment_angle);
        self.writer
            .write_bit_double(style.second_segment_angle);

        // Leader
        self.writer
            .write_bit_short(style.path_type as i16);
        self.writer.write_cm_color(&style.line_color);

        let lt = style.line_type_handle.unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, lt.value());
        self.writer
            .write_bit_long(style.line_weight.as_i16() as i32);

        self.writer.write_bit(style.enable_landing);
        self.writer.write_bit_double(style.landing_gap);
        self.writer.write_bit(style.enable_dogleg);
        self.writer
            .write_bit_double(style.landing_distance);

        self.writer.write_variable_text(&style.description);

        // Arrowhead
        let ah = style.arrowhead_handle.unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, ah.value());
        self.writer.write_bit_double(style.arrowhead_size);

        // Default text
        self.writer.write_variable_text(&style.default_text);

        // Text style
        let ts = style.text_style_handle.unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, ts.value());

        // Text attachments
        self.writer
            .write_bit_short(style.text_left_attachment as i16);
        self.writer
            .write_bit_short(style.text_right_attachment as i16);
        self.writer
            .write_bit_short(style.text_angle_type as i16);
        self.writer
            .write_bit_short(style.text_alignment as i16);
        self.writer.write_cm_color(&style.text_color);
        self.writer.write_bit_double(style.text_height);
        self.writer.write_bit(style.text_frame);
        self.writer.write_bit(style.text_always_left);

        self.writer
            .write_bit_double(style.align_space);

        // Block
        let bc = style.block_content_handle.unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, bc.value());
        self.writer
            .write_cm_color(&style.block_content_color);
        self.writer
            .write_bit_double(style.block_content_scale_x);
        self.writer
            .write_bit_double(style.block_content_scale_y);
        self.writer
            .write_bit_double(style.block_content_scale_z);
        self.writer.write_bit(style.enable_block_scale);
        self.writer
            .write_bit_double(style.block_content_rotation);
        self.writer.write_bit(style.enable_block_rotation);
        self.writer
            .write_bit_short(style.block_content_connection as i16);

        self.writer.write_bit_double(style.scale_factor);
        self.writer.write_bit(style.property_changed);
        self.writer.write_bit(style.is_annotative);
        self.writer
            .write_bit_double(style.break_gap_size);

        // R2010+ additional fields
        if self.version.r2010_plus() {
            self.writer
                .write_bit_short(style.text_attachment_direction as i16);
            self.writer
                .write_bit_short(style.text_top_attachment as i16);
            self.writer
                .write_bit_short(style.text_bottom_attachment as i16);
        }

        // R2013+ undocumented flag (DXF code 298)
        if self.version.r2013_plus(self.dxf_version) {
            self.writer.write_bit(style.unknown_flag_298);
        }

        self.register_object(style.handle);
    }

    // ── Image Definition ────────────────────────────────────────────

    fn write_image_definition(&mut self, def: &ImageDefinition) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("IMAGEDEF", common::OBJ_IMAGEDEF);
        self.write_common_non_entity_data(
            type_code,
            def.handle,
            def.owner,
            &[],
            &None,
        );

        self.writer.write_bit_long(def.class_version);
        self.writer
            .write_2raw_double(crate::types::Vector2::new(
                def.size_in_pixels.0 as f64,
                def.size_in_pixels.1 as f64,
            ));
        self.writer.write_variable_text(&def.file_name);
        self.writer
            .write_bit(def.is_loaded);
        self.writer
            .write_byte(def.resolution_unit as u8);
        self.writer
            .write_2raw_double(crate::types::Vector2::new(
                def.pixel_size.0,
                def.pixel_size.1,
            ));

        self.register_object(def.handle);
    }

    // ── Image Definition Reactor ────────────────────────────────────

    fn write_image_definition_reactor(&mut self, reactor: &ImageDefinitionReactor) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("IMAGEDEF_REACTOR", common::OBJ_IMAGEDEFREACTOR);
        self.write_common_non_entity_data(
            type_code,
            reactor.handle,
            reactor.owner,
            &[],
            &None,
        );

        self.writer.write_bit_long(0); // class version

        // C# reference does NOT write an image_handle here
        // (the reader gets this from the reactor's owner relationship)

        self.register_object(reactor.handle);
    }

    // ── Scale ───────────────────────────────────────────────────────

    fn write_scale(&mut self, scale: &Scale) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("SCALE", common::OBJ_SCALE);
        self.write_common_non_entity_data(
            type_code,
            scale.handle,
            scale.owner_handle,
            &[],
            &None,
        );

        self.writer.write_bit_short(0); // unknown BS
        self.writer.write_variable_text(&scale.name);
        self.writer.write_bit_double(scale.paper_units);
        self.writer.write_bit_double(scale.drawing_units);
        self.writer.write_bit(scale.is_unit_scale);

        self.register_object(scale.handle);
    }

    // ── Sort Entities Table ─────────────────────────────────────────

    fn write_sort_entities_table(&mut self, table: &SortEntitiesTable) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("SORTENTSTABLE", common::OBJ_SORTENTSTABLE);
        self.write_common_non_entity_data(
            type_code,
            table.handle,
            table.owner_handle,
            &[],
            &None,
        );

        let entries: Vec<_> = table.entries().collect();
        self.writer.write_bit_long(entries.len() as i32);

        // Sort handles are stored inline in the DATA section (one per entry);
        // the owner block and the sorted entity handles follow in the handle
        // stream (owner first, then one per entry). Mirrors `read_sort_entities_
        // table`. (#146)
        for entry in &entries {
            self.writer
                .write_main_handle(DwgReferenceType::SoftPointer, entry.sort_handle.value());
        }
        self.writer
            .write_handle(DwgReferenceType::HardPointer, table.block_owner_handle.value());
        for entry in &entries {
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, entry.entity_handle.value());
        }

        self.register_object(table.handle);
    }

    // ── XRecord ─────────────────────────────────────────────────────

    fn write_xrecord(&mut self, xrec: &XRecord) {
        self.write_common_non_entity_data(
            common::OBJ_XRECORD,
            xrec.handle,
            xrec.owner,
            &[],
            &None,
        );

        // Write raw data bytes first (per spec: data before cloning flags)
        if !xrec.raw_data.is_empty() {
            self.writer.write_bit_long(xrec.raw_data.len() as i32);
            for &b in &xrec.raw_data {
                self.writer.write_byte(b);
            }
        } else {
            self.writer.write_bit_long(0);
        }

        // R2000+: Cloning flags (valid range 0..5; enum already constrains to valid values)
        self.writer.write_bit_short(xrec.cloning_flags.to_value());

        self.register_object(xrec.handle);
    }

    // ── Raster Variables ────────────────────────────────────────────

    fn write_raster_variables(&mut self, rv: &RasterVariables) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("RASTERVARIABLES", common::OBJ_RASTERVARIABLES);
        self.write_common_non_entity_data(
            type_code,
            rv.handle,
            rv.owner,
            &[],
            &None,
        );

        self.writer.write_bit_long(rv.class_version);
        self.writer.write_bit_short(rv.display_image_frame);
        self.writer.write_bit_short(rv.image_quality);
        self.writer.write_bit_short(rv.units);

        self.register_object(rv.handle);
    }

    // ── Spatial Filter (XCLIP clip boundary) ────────────────────────

    fn write_spatial_filter(&mut self, sf: &SpatialFilter) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("SPATIAL_FILTER", common::OBJ_SPATIALFILTER);
        self.write_common_non_entity_data(
            type_code,
            sf.handle,
            sf.owner,
            &[],
            &None,
        );

        self.writer.write_bit_short(sf.boundary_points.len() as i16);
        for p in &sf.boundary_points {
            self.writer.write_2raw_double(*p);
        }
        self.writer.write_3bit_double(sf.normal);
        self.writer.write_3bit_double(sf.origin);
        self.writer.write_bit_short(sf.display_enabled as i16);
        self.writer.write_bit_short(sf.front_clip.is_some() as i16);
        if let Some(d) = sf.front_clip {
            self.writer.write_bit_double(d);
        }
        self.writer.write_bit_short(sf.back_clip.is_some() as i16);
        if let Some(d) = sf.back_clip {
            self.writer.write_bit_double(d);
        }
        for v in matrix_to_row_major(&sf.inverse_block_transform) {
            self.writer.write_bit_double(v);
        }
        for v in matrix_to_row_major(&sf.clip_bound_transform) {
            self.writer.write_bit_double(v);
        }

        self.register_object(sf.handle);
    }

    // ── PlaceHolder ─────────────────────────────────────────────────

    fn write_placeholder(&mut self, ph: &PlaceHolder) {
        self.write_common_non_entity_data(
            common::OBJ_PLACEHOLDER,
            ph.handle,
            ph.owner,
            &[],
            &None,
        );

        self.register_object(ph.handle);
    }

    // ── BookColor ───────────────────────────────────────────────────

    fn write_book_color(&mut self, bc: &BookColor) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("DBCOLOR", common::OBJ_DBCOLOR);
        self.write_common_non_entity_data(
            type_code,
            bc.handle,
            bc.owner,
            &[],
            &None,
        );

        self.writer.write_variable_text(&bc.color_name);
        self.writer.write_variable_text(&bc.book_name);

        self.register_object(bc.handle);
    }

    // ── Wipeout Variables ───────────────────────────────────────────

    fn write_wipeout_variables(&mut self, wv: &WipeoutVariables) {
        // UNLISTED type — always use DXF class number (500+)
        let type_code = self.class_type_code("WIPEOUTVARIABLES", common::OBJ_WIPEOUTVARIABLES);
        self.write_common_non_entity_data(
            type_code,
            wv.handle,
            wv.owner,
            &[],
            &None,
        );

        self.writer.write_bit_short(wv.display_frame);

        self.register_object(wv.handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::CadDocument;

    #[test]
    fn write_empty_dictionary() {
        let doc = CadDocument::new();
        let mut writer = DwgObjectWriter::new(&doc).unwrap();
        let dict = Dictionary::default();
        writer.write_dictionary(&dict);
        assert!(!writer.output.is_empty());
    }

    #[test]
    fn write_dictionary_with_entries() {
        let doc = CadDocument::new();
        let mut writer = DwgObjectWriter::new(&doc).unwrap();
        let mut dict = Dictionary::new();
        dict.handle = Handle::new(0x10);
        dict.add_entry("TestEntry".to_string(), Handle::new(0x20));
        writer.write_dictionary(&dict);
        assert!(!writer.output.is_empty());
        // Should have enqueued the child handle
        assert!(writer.object_queue.contains(&Handle::new(0x20)));
    }
}
