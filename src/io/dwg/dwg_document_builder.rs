//! DWG Document Builder — maps raw DWG parsed data into CadDocument.
//!
//! This module bridges the gap between the low-level object readers
//! (which produce `*Data` structs) and the high-level domain model
//! (entities, objects, tables in `CadDocument`).
//!
//! ## Two-Pass Architecture
//!
//! **Pass 1 (Tables):** Read all table entries (layers, block headers,
//! text styles, linetypes) and build handle→name lookup maps.
//!
//! **Pass 2 (Entities & Objects):** Read entities and objects, resolving
//! handle references (e.g., layer_handle → layer name, block_handle →
//! block name) using the maps built in Pass 1.

use std::collections::HashMap;
use crate::document::CadDocument;
use crate::entities::*;
use crate::entities::EntityCommon;
use crate::notification::{NotificationCollection, NotificationType};
use crate::types::Handle;
use crate::types::LineWeight;
use crate::io::dwg::dwg_stream_readers::object_reader::{
    DwgObjectReader, EntityCommonData,
};
use crate::io::dwg::dwg_stream_readers::object_reader::common::*;
use crate::io::dwg::dwg_stream_readers::object_reader::entities;
use crate::io::dwg::dwg_stream_readers::object_reader::objects;
use crate::io::dwg::dwg_stream_readers::object_reader::tables;

/// Pending vertex data collected during Pass 2, keyed by owner (parent polyline) handle.
enum PendingVertex {
    V2D(entities::Vertex2DData),
    V3D(entities::Vertex3DData),
    PfaceFace(entities::PfaceFaceData),
}

/// Pending polyline entities awaiting vertex assembly.
struct PendingPolylines {
    /// Vertex data keyed by owner (parent polyline) handle.
    vertices: HashMap<u64, Vec<PendingVertex>>,
    /// Polyline entities awaiting vertex assembly, keyed by their handle.
    polylines: Vec<(u64, EntityType)>,
}

/// Handle-to-name resolution maps built from table entries.
struct HandleMaps {
    /// handle → layer name
    layers: HashMap<u64, String>,
    /// handle → block name
    blocks: HashMap<u64, String>,
    /// handle → text style name
    text_styles: HashMap<u64, String>,
    /// handle → linetype name
    linetypes: HashMap<u64, String>,
    /// handle → dimension style name
    dim_styles: HashMap<u64, String>,
}

impl HandleMaps {
    fn new() -> Self {
        Self {
            layers: HashMap::new(),
            blocks: HashMap::new(),
            text_styles: HashMap::new(),
            linetypes: HashMap::new(),
            dim_styles: HashMap::new(),
        }
    }

    fn layer_name(&self, handle: u64) -> String {
        self.layers.get(&handle).cloned().unwrap_or_else(|| "0".to_string())
    }

    fn block_name(&self, handle: u64) -> String {
        self.blocks.get(&handle).cloned().unwrap_or_else(|| format!("*U{}", handle))
    }

    fn style_name(&self, handle: u64) -> String {
        self.text_styles.get(&handle).cloned().unwrap_or_else(|| "STANDARD".to_string())
    }

    #[allow(dead_code)]
    fn dimstyle_name(&self, handle: u64) -> String {
        self.dim_styles.get(&handle).cloned().unwrap_or_else(|| "Standard".to_string())
    }
}

/// Builds a `CadDocument` from parsed DWG object data.
pub struct DwgDocumentBuilder {
    obj_reader: DwgObjectReader,
    /// Whether to use failsafe mode (report skipped records via notifications).
    failsafe: bool,
    /// Notifications collected during building.
    notifications: NotificationCollection,
}

impl DwgDocumentBuilder {
    /// Create a new builder wrapping the object reader.
    pub fn new(obj_reader: DwgObjectReader) -> Self {
        Self {
            obj_reader,
            failsafe: false,
            notifications: NotificationCollection::new(),
        }
    }

    /// Enable or disable failsafe mode.
    ///
    /// When enabled, skipped records are reported as notifications
    /// instead of being silently lost.
    pub fn set_failsafe(&mut self, failsafe: bool) {
        self.failsafe = failsafe;
    }

    /// Build the document by iterating all handles and dispatching objects.
    ///
    /// Uses a two-pass approach:
    /// 1. Read table entries → build handle→name maps
    /// 2. Read entities and objects → resolve handle references
    ///
    /// Returns collected notifications (skipped records, warnings).
    pub fn build(mut self, document: &mut CadDocument) -> NotificationCollection {
        let mut handles = self.obj_reader.handles();
        // Sort handles numerically so that entity records are processed in
        // allocation order.  This ensures polyline vertex records are
        // encountered in the correct sequence (the writer allocates
        // sequential handles for child entities).
        handles.sort_unstable();
        let mut skipped_pass1 = 0u32;
        let mut skipped_pass2 = 0u32;
        let total_handles = handles.len();

        // Build class_number → internal type code mapping for non-fixed types.
        // The DWG binary uses class numbers (500+) for object types defined in
        // the CLASSES section.  We translate these to our internal OBJ_*
        // constants so the match statements work correctly.
        let class_map = Self::build_class_type_map(document);

        // ── Pass 1: Build handle→name maps from table entries ──────────
        //
        // In addition to building handle→name lookup maps (for Pass 2
        // entity resolution), we now also create full domain objects
        // (Layer, BlockRecord, TextStyle, LineType, DimStyle) and
        // populate the document tables.  This mirrors what the DXF
        // reader does in its TABLES section reader.
        let mut maps = HandleMaps::new();

        // Parsed table entries collected for post-loop domain-object creation.
        // We collect first and create domain objects after the loop so that
        // cross-references (e.g. layer → linetype name) can be resolved
        // using the fully-populated handle→name maps.
        enum ParsedEntry {
            Layer(u64, tables::LayerData),
            Block(u64, tables::BlockHeaderData),
            Style(u64, tables::TextStyleData),
            Ltype(u64, tables::LinetypeData),
            DimStyle(u64, tables::DimStyleData),
        }
        let mut parsed_entries: Vec<ParsedEntry> = Vec::new();

        for &handle in &handles {
            let offset = match self.obj_reader.offset_for(handle) {
                Some(o) if o >= 0 => o,
                _ => continue,
            };
            let (raw_type_code, mut reader) = match self.obj_reader.read_record_at(offset as usize) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let type_code = Self::resolve_type_code(raw_type_code, &class_map);

            if is_table_type(type_code) {
                // Wrap in catch_unwind to survive corrupt/misaligned records
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let non_entity = self.obj_reader.read_common_non_entity_data(&mut reader, type_code);
                    let obj_handle = non_entity.common.handle;
                    (obj_handle, type_code)
                }));
                let (obj_handle, type_code) = match result {
                    Ok(v) => v,
                    Err(_) => {
                        skipped_pass1 += 1;
                        self.notifications.notify(
                            NotificationType::Error,
                            format!(
                                "Skipped corrupt table record at handle {:#X} (panic in common data)",
                                handle
                            ),
                        );
                        continue;
                    }
                };

                let table_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match type_code {
                        OBJ_LAYER => {
                            let data = tables::read_layer(
                                &mut reader,
                                self.obj_reader.version(),
                                self.obj_reader.dxf_version(),
                            );
                            Some(ParsedEntry::Layer(obj_handle, data))
                        },
                        OBJ_BLOCK_HEADER => {
                            let data = tables::read_block_header(
                                &mut reader,
                                self.obj_reader.version(),
                            );
                            Some(ParsedEntry::Block(obj_handle, data))
                        },
                        OBJ_STYLE => {
                            let data = tables::read_text_style(
                                &mut reader,
                                self.obj_reader.version(),
                            );
                            Some(ParsedEntry::Style(obj_handle, data))
                        },
                        OBJ_LTYPE => {
                            let data = tables::read_linetype(
                                &mut reader,
                                self.obj_reader.version(),
                            );
                            Some(ParsedEntry::Ltype(obj_handle, data))
                        },
                        OBJ_DIMSTYLE => {
                            let data = tables::read_dimstyle(
                                &mut reader,
                                self.obj_reader.version(),
                                self.obj_reader.dxf_version(),
                            );
                            Some(ParsedEntry::DimStyle(obj_handle, data))
                        },
                        _ => None,
                    }
                }));
                match table_result {
                    Ok(Some(entry)) => {
                        // Populate handle→name maps (needed by Pass 2)
                        match &entry {
                            ParsedEntry::Layer(h, data) => { maps.layers.insert(*h, data.name.clone()); },
                            ParsedEntry::Block(h, data) => { maps.blocks.insert(*h, data.name.clone()); },
                            ParsedEntry::Style(h, data) => { maps.text_styles.insert(*h, data.name.clone()); },
                            ParsedEntry::Ltype(h, data) => { maps.linetypes.insert(*h, data.name.clone()); },
                            ParsedEntry::DimStyle(h, data) => { maps.dim_styles.insert(*h, data.name.clone()); },
                        }
                        parsed_entries.push(entry);
                    }
                    Ok(None) => {}
                    Err(_) => {
                        skipped_pass1 += 1;
                        self.notifications.notify(
                            NotificationType::Error,
                            format!(
                                "Skipped corrupt table record at handle {:#X}, type_code={}",
                                handle, type_code
                            ),
                        );
                    }
                }
            }
        }

        // ── Post-Pass 1: Populate document tables from parsed data ─────
        //
        // Now that all handle→name maps are complete, create domain objects
        // with resolved cross-references and add them to the document.
        // Remove any default entries first to avoid duplicates.
        for entry in &parsed_entries {
            match entry {
                ParsedEntry::Layer(h, data) => {
                    let mut layer = crate::tables::Layer::new(&data.name);
                    layer.handle = Handle::from(*h);
                    layer.flags.frozen = data.frozen;
                    layer.flags.off = data.off;
                    layer.flags.locked = data.locked;
                    layer.is_plottable = data.plottable;
                    layer.line_weight = LineWeight::from_value(data.line_weight);
                    layer.color = data.color;
                    // Resolve linetype handle → name
                    layer.line_type = maps.linetypes.get(&data.linetype_handle)
                        .cloned()
                        .unwrap_or_else(|| "Continuous".to_string());
                    // Remove default entry if it exists, then add
                    let _ = document.layers.remove(&data.name);
                    let _ = document.layers.add(layer);
                },
                ParsedEntry::Block(h, data) => {
                    let mut br = crate::tables::BlockRecord::new(&data.name);
                    br.handle = Handle::from(*h);
                    br.flags.anonymous = data.anonymous;
                    br.flags.has_attributes = data.has_attributes;
                    br.flags.is_xref = data.is_xref;
                    br.flags.is_xref_overlay = data.is_xref_overlay;
                    br.block_entity_handle = Handle::from(data.block_entity_handle);
                    br.block_end_handle = Handle::from(data.endblk_handle);
                    br.units = data.units.unwrap_or(0);
                    br.explodable = data.explodable.unwrap_or(true);
                    br.scale_uniformly = data.scale_uniformly.map(|v| v != 0).unwrap_or(false);
                    if let Some(layout_h) = data.layout_handle {
                        br.layout = Handle::from(layout_h);
                    }
                    // Update header handles for model/paper space
                    if data.name.eq_ignore_ascii_case("*Model_Space") {
                        document.header.model_space_block_handle = br.handle;
                    } else if data.name.eq_ignore_ascii_case("*Paper_Space") {
                        document.header.paper_space_block_handle = br.handle;
                    }
                    // Remove default entry if it exists, then add
                    let _ = document.block_records.remove(&data.name);
                    let _ = document.block_records.add(br);
                },
                ParsedEntry::Style(h, data) => {
                    let mut style = crate::tables::TextStyle::new(&data.name);
                    style.handle = Handle::from(*h);
                    style.height = data.height;
                    style.width_factor = data.width_factor;
                    style.oblique_angle = data.oblique_angle;
                    style.last_height = data.last_height;
                    style.font_file = data.font_file.clone();
                    style.big_font_file = data.big_font_file.clone();
                    style.flags.backward = (data.generation & 2) != 0;
                    style.flags.upside_down = (data.generation & 4) != 0;
                    let _ = document.text_styles.remove(&data.name);
                    let _ = document.text_styles.add(style);
                },
                ParsedEntry::Ltype(h, data) => {
                    let mut lt = crate::tables::LineType::new(&data.name);
                    lt.handle = Handle::from(*h);
                    lt.description = data.description.clone();
                    lt.pattern_length = data.pattern_length;
                    lt.elements = data.segments.iter().map(|s| {
                        crate::tables::LineTypeElement { length: s.length }
                    }).collect();
                    let _ = document.line_types.remove(&data.name);
                    let _ = document.line_types.add(lt);
                },
                ParsedEntry::DimStyle(h, data) => {
                    let mut ds = crate::tables::DimStyle::new(&data.name);
                    ds.handle = Handle::from(*h);
                    ds.dimscale = data.dimscale;
                    ds.dimasz = data.dimasz;
                    ds.dimexo = data.dimexo;
                    ds.dimdli = data.dimdli;
                    ds.dimexe = data.dimexe;
                    ds.dimrnd = data.dimrnd;
                    ds.dimdle = data.dimdle;
                    ds.dimtp = data.dimtp;
                    ds.dimtm = data.dimtm;
                    ds.dimtol = data.dimtol;
                    ds.dimlim = data.dimlim;
                    ds.dimtih = data.dimtih;
                    ds.dimtoh = data.dimtoh;
                    ds.dimse1 = data.dimse1;
                    ds.dimse2 = data.dimse2;
                    ds.dimtad = data.dimtad;
                    ds.dimzin = data.dimzin;
                    ds.dimtxt = data.dimtxt;
                    ds.dimcen = data.dimcen;
                    ds.dimtsz = data.dimtsz;
                    ds.dimaltf = data.dimaltf;
                    ds.dimlfac = data.dimlfac;
                    ds.dimtvp = data.dimtvp;
                    ds.dimtfac = data.dimtfac;
                    ds.dimgap = data.dimgap;
                    ds.dimalt = data.dimalt;
                    ds.dimaltd = data.dimaltd;
                    ds.dimtofl = data.dimtofl;
                    ds.dimsah = data.dimsah;
                    ds.dimtix = data.dimtix;
                    ds.dimsoxd = data.dimsoxd;
                    ds.dimclrd = data.dimclrd.index().unwrap_or(0) as i16;
                    ds.dimclre = data.dimclre.index().unwrap_or(0) as i16;
                    ds.dimclrt = data.dimclrt.index().unwrap_or(0) as i16;
                    ds.dimsd1 = data.dimsd1;
                    ds.dimsd2 = data.dimsd2;
                    ds.dimtolj = data.dimtolj;
                    ds.dimtzin = data.dimtzin;
                    ds.dimupt = data.dimupt;
                    ds.dimfit = data.dimfit;
                    ds.dimlwd = data.dimlwd;
                    ds.dimlwe = data.dimlwe;
                    ds.dimpost = data.dimpost.clone();
                    ds.dimapost = data.dimapost.clone();
                    ds.dimaltrnd = data.dimaltrnd;
                    ds.dimadec = data.dimadec;
                    ds.dimdec = data.dimdec;
                    ds.dimtdec = data.dimtdec;
                    ds.dimaltu = data.dimaltu;
                    ds.dimalttd = data.dimalttd;
                    ds.dimaunit = data.dimaunit;
                    ds.dimfrac = data.dimfrac;
                    ds.dimlunit = data.dimlunit;
                    ds.dimdsep = data.dimdsep;
                    ds.dimtmove = data.dimtmove;
                    ds.dimjust = data.dimjust;
                    ds.dimaltz = data.dimaltz;
                    ds.dimalttz = data.dimalttz;
                    // Resolve text style handle
                    if data.dimtxsty_handle != 0 {
                        ds.dimtxsty_handle = Handle::from(data.dimtxsty_handle);
                        ds.dimtxsty = maps.text_styles.get(&data.dimtxsty_handle)
                            .cloned()
                            .unwrap_or_else(|| "Standard".to_string());
                    }
                    let _ = document.dim_styles.remove(&data.name);
                    let _ = document.dim_styles.add(ds);
                },
            }
        }

        // ── Pass 2: Read entities and non-table objects ────────────────
        let mut pending = PendingPolylines {
            vertices: HashMap::new(),
            polylines: Vec::new(),
        };
        for &handle in &handles {
            let offset = match self.obj_reader.offset_for(handle) {
                Some(o) if o >= 0 => o,
                _ => continue,
            };
            let (raw_type_code, reader) = match self.obj_reader.read_record_at(offset as usize) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let type_code = Self::resolve_type_code(raw_type_code, &class_map);

            // Wrap per-object processing in catch_unwind to survive
            // corrupt or misaligned records without crashing the entire read.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.process_pass2_record(handle, type_code, reader, document, &maps, &mut pending);
            }));
            if let Err(ref _e) = result {
                skipped_pass2 += 1;
                self.notifications.notify(
                    NotificationType::Error,
                    format!(
                        "Skipped corrupt record at handle {:#X}, type_code={} (panic recovered)",
                        handle, type_code
                    ),
                );
                continue;
            }
        }

        // ── Post-pass: Assemble polyline vertices and add to document ──
        for (poly_handle, mut entity) in pending.polylines {
            if let Some(verts) = pending.vertices.remove(&poly_handle) {
                match &mut entity {
                    EntityType::Polyline2D(ref mut e) => {
                        e.vertices = verts.into_iter().filter_map(|v| {
                            if let PendingVertex::V2D(d) = v {
                                Some(crate::entities::polyline::Vertex2D {
                                    location: crate::types::Vector3::new(d.x, d.y, d.z),
                                    flags: crate::entities::polyline::VertexFlags::from_bits(d.flags),
                                    start_width: d.start_width,
                                    end_width: d.end_width,
                                    bulge: d.bulge,
                                    curve_tangent: d.tangent_dir,
                                    id: d.vertex_id,
                                })
                            } else { None }
                        }).collect();
                    }
                    EntityType::Polyline3D(ref mut e) => {
                        e.vertices = verts.into_iter().filter_map(|v| {
                            if let PendingVertex::V3D(d) = v {
                                Some(crate::entities::polyline3d::Vertex3DPolyline {
                                    handle: Handle::NULL,
                                    layer: String::new(),
                                    position: d.position,
                                    flags: d.flags as i32,
                                })
                            } else { None }
                        }).collect();
                    }
                    EntityType::PolyfaceMesh(ref mut e) => {
                        for v in verts {
                            match v {
                                PendingVertex::V3D(d) => {
                                    e.vertices.push(crate::entities::polyface_mesh::PolyfaceVertex {
                                        common: EntityCommon::default(),
                                        location: d.position,
                                        flags: crate::entities::polyface_mesh::PolyfaceVertexFlags::POLYFACE_MESH,
                                        bulge: 0.0,
                                        start_width: 0.0,
                                        end_width: 0.0,
                                        curve_tangent: 0.0,
                                        id: 0,
                                    });
                                }
                                PendingVertex::PfaceFace(f) => {
                                    e.faces.push(crate::entities::polyface_mesh::PolyfaceFace {
                                        common: EntityCommon::default(),
                                        flags: crate::entities::polyface_mesh::PolyfaceVertexFlags::NONE,
                                        index1: f.index1,
                                        index2: f.index2,
                                        index3: f.index3,
                                        index4: f.index4,
                                        color: None,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            let _ = document.add_entity(entity);
        }

        // Summary notification
        let total_skipped = skipped_pass1 + skipped_pass2;
        if total_skipped > 0 {
            self.notifications.notify(
                NotificationType::Warning,
                format!(
                    "DWG build summary: {} of {} handles processed, {} records skipped ({} table, {} entity/object)",
                    total_handles as u32 - total_skipped,
                    total_handles,
                    total_skipped,
                    skipped_pass1,
                    skipped_pass2,
                ),
            );
        }

        self.notifications
    }

    /// Process a single object record in Pass 2.
    fn process_pass2_record(
        &self,
        handle: u64,
        type_code: i16,
        mut reader: crate::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader,
        document: &mut CadDocument,
        maps: &HandleMaps,
        pending: &mut PendingPolylines,
    ) {
        if is_entity_type(type_code) {
            let entity_data = self.obj_reader.read_common_entity_data(&mut reader, type_code);
            let entity_common = map_entity_common(&entity_data, maps);

            match type_code {
                // ── Simple entities ────────────────────────────────
                OBJ_LINE => {
                    let data = entities::read_line(&mut reader, self.obj_reader.version());
                    let mut e = Line::new();
                    e.common = entity_common;
                    e.start = data.start;
                    e.end = data.end;
                    e.thickness = data.thickness;
                    e.normal = data.normal;
                    let _ = document.add_entity(EntityType::Line(e));
                },
                OBJ_POINT => {
                    let data = entities::read_point(&mut reader);
                    let mut e = Point::new();
                    e.common = entity_common;
                    e.location = data.location;
                    e.thickness = data.thickness;
                    e.normal = data.normal;
                    let _ = document.add_entity(EntityType::Point(e));
                },
                OBJ_CIRCLE => {
                    let data = entities::read_circle(&mut reader);
                    let mut e = Circle::new();
                    e.common = entity_common;
                    e.center = data.center;
                    e.radius = data.radius;
                    e.thickness = data.thickness;
                    e.normal = data.normal;
                    let _ = document.add_entity(EntityType::Circle(e));
                },
                OBJ_ARC => {
                    let data = entities::read_arc(&mut reader);
                    let mut e = Arc::new();
                    e.common = entity_common;
                    e.center = data.center;
                    e.radius = data.radius;
                    e.start_angle = data.start_angle;
                    e.end_angle = data.end_angle;
                    e.thickness = data.thickness;
                    e.normal = data.normal;
                    let _ = document.add_entity(EntityType::Arc(e));
                },
                OBJ_ELLIPSE => {
                    let data = entities::read_ellipse(&mut reader);
                    let mut e = Ellipse::new();
                    e.common = entity_common;
                    e.center = data.center;
                    e.major_axis = data.major_axis;
                    e.minor_axis_ratio = data.minor_axis_ratio;
                    e.start_parameter = data.start_parameter;
                    e.end_parameter = data.end_parameter;
                    e.normal = data.normal;
                    let _ = document.add_entity(EntityType::Ellipse(e));
                },
                OBJ_RAY => {
                    let data = entities::read_ray(&mut reader);
                    let mut e = Ray::new(data.base_point, data.direction);
                    e.common = entity_common;
                    let _ = document.add_entity(EntityType::Ray(e));
                },
                OBJ_XLINE => {
                    let data = entities::read_xline(&mut reader);
                    let mut e = XLine::new(data.base_point, data.direction);
                    e.common = entity_common;
                    let _ = document.add_entity(EntityType::XLine(e));
                },
                OBJ_SOLID | OBJ_TRACE => {
                    let data = entities::read_solid(&mut reader);
                    let z = data.elevation;
                    let mut e = Solid::new(
                        crate::types::Vector3::new(data.first_corner.x, data.first_corner.y, z),
                        crate::types::Vector3::new(data.second_corner.x, data.second_corner.y, z),
                        crate::types::Vector3::new(data.third_corner.x, data.third_corner.y, z),
                        crate::types::Vector3::new(data.fourth_corner.x, data.fourth_corner.y, z),
                    );
                    e.common = entity_common;
                    e.thickness = data.thickness;
                    e.normal = data.normal;
                    let _ = document.add_entity(EntityType::Solid(e));
                },
                OBJ_3DFACE => {
                    let data = entities::read_face3d(&mut reader, self.obj_reader.version());
                    let mut e = Face3D::new(
                        data.first_corner,
                        data.second_corner,
                        data.third_corner,
                        data.fourth_corner,
                    );
                    e.common = entity_common;
                    let _ = document.add_entity(EntityType::Face3D(e));
                },
                OBJ_SHAPE => {
                    let data = entities::read_shape(&mut reader);
                    let mut e = Shape::new();
                    e.common = entity_common;
                    e.insertion_point = data.insertion_point;
                    e.size = data.size;
                    e.rotation = data.rotation;
                    e.relative_x_scale = data.relative_x_scale;
                    e.oblique_angle = data.oblique_angle;
                    e.thickness = data.thickness;
                    e.shape_number = data.shape_number as i32;
                    e.normal = data.normal;
                    e.style_handle = Some(Handle::from(data.style_handle));
                    let _ = document.add_entity(EntityType::Shape(e));
                },

                // ── Moderate entities ──────────────────────────────
                OBJ_INSERT | OBJ_MINSERT => {
                    let data = entities::read_insert(&mut reader, self.obj_reader.version());
                    let block_name = maps.block_name(data.block_handle);
                    let mut e = Insert::new(block_name, data.insert_point);
                    e.common = entity_common;
                    e.x_scale = data.x_scale;
                    e.y_scale = data.y_scale;
                    e.z_scale = data.z_scale;
                    e.rotation = data.rotation;
                    e.normal = data.normal;
                    let _ = document.add_entity(EntityType::Insert(e));
                },
                OBJ_LWPOLYLINE => {
                    let data = entities::read_lwpolyline(&mut reader, self.obj_reader.version());
                    let mut e = LwPolyline::new();
                    e.common = entity_common;
                    e.vertices = data.vertices.into_iter().map(|v| {
                        crate::entities::lwpolyline::LwVertex {
                            location: crate::types::Vector2::new(v.x, v.y),
                            start_width: v.start_width,
                            end_width: v.end_width,
                            bulge: v.bulge,
                        }
                    }).collect();
                    e.elevation = data.elevation;
                    e.thickness = data.thickness;
                    e.constant_width = data.constant_width;
                    e.normal = data.normal;
                    e.is_closed = (data.flag & 0x200) != 0;
                    let _ = document.add_entity(EntityType::LwPolyline(e));
                },
                OBJ_SPLINE => {
                    let data = entities::read_spline(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut e = Spline::new();
                    e.common = entity_common;
                    e.degree = data.degree;
                    e.flags.rational = data.rational;
                    e.flags.closed = data.closed;
                    e.flags.periodic = data.periodic;
                    e.knots = data.knots;
                    e.control_points = data.control_points;
                    e.weights = data.weights;
                    e.fit_points = data.fit_points;
                    let _ = document.add_entity(EntityType::Spline(e));
                },
                OBJ_TEXT => {
                    let data = entities::read_text(&mut reader, self.obj_reader.version());
                    let mut e = Text::new();
                    e.common = entity_common;
                    e.value = data.value;
                    e.insertion_point = data.insertion_point;
                    e.height = data.height;
                    e.horizontal_alignment = match data.horizontal_alignment {
                        1 => TextHorizontalAlignment::Center,
                        2 => TextHorizontalAlignment::Right,
                        3 => TextHorizontalAlignment::Aligned,
                        4 => TextHorizontalAlignment::Middle,
                        5 => TextHorizontalAlignment::Fit,
                        _ => TextHorizontalAlignment::Left,
                    };
                    e.vertical_alignment = match data.vertical_alignment {
                        1 => TextVerticalAlignment::Bottom,
                        2 => TextVerticalAlignment::Middle,
                        3 => TextVerticalAlignment::Top,
                        _ => TextVerticalAlignment::Baseline,
                    };
                    // Only set alignment_point when alignment mode actually uses it
                    e.alignment_point = if data.horizontal_alignment != 0 || data.vertical_alignment != 0 {
                        Some(data.alignment_point)
                    } else {
                        None
                    };
                    e.rotation = data.rotation;
                    e.oblique_angle = data.oblique_angle;
                    e.width_factor = data.width_factor;
                    e.normal = data.normal;
                    e.style = maps.style_name(data.style_handle);
                    let _ = document.add_entity(EntityType::Text(e));
                },
                OBJ_MTEXT => {
                    let data = entities::read_mtext(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut e = MText::new();
                    e.common = entity_common;
                    e.value = data.value;
                    e.insertion_point = data.insertion_point;
                    e.height = data.height;
                    e.rectangle_width = data.rectangle_width;
                    e.normal = data.normal;
                    e.attachment_point = match data.attachment_point {
                        2 => AttachmentPoint::TopCenter,
                        3 => AttachmentPoint::TopRight,
                        4 => AttachmentPoint::MiddleLeft,
                        5 => AttachmentPoint::MiddleCenter,
                        6 => AttachmentPoint::MiddleRight,
                        7 => AttachmentPoint::BottomLeft,
                        8 => AttachmentPoint::BottomCenter,
                        9 => AttachmentPoint::BottomRight,
                        _ => AttachmentPoint::TopLeft,
                    };
                    e.drawing_direction = match data.drawing_direction {
                        2 => DrawingDirection::TopToBottom,
                        3 => DrawingDirection::ByStyle,
                        _ => DrawingDirection::LeftToRight,
                    };
                    // Compute rotation from x_direction vector
                    e.rotation = data.x_direction.y.atan2(data.x_direction.x);
                    e.line_spacing_factor = data.linespacing_factor;
                    e.style = maps.style_name(data.style_handle);
                    let _ = document.add_entity(EntityType::MText(e));
                },
                OBJ_LEADER => {
                    let data = entities::read_leader(&mut reader, self.obj_reader.version());
                    let mut e = Leader::new();
                    e.common = entity_common;
                    e.vertices = data.vertices;
                    e.normal = data.normal;
                    e.horizontal_direction = data.horizontal_direction;
                    e.annotation_handle = Handle::from(data.annotation_handle);
                    let _ = document.add_entity(EntityType::Leader(e));
                },
                OBJ_TOLERANCE => {
                    let data = entities::read_tolerance(&mut reader, self.obj_reader.version());
                    let mut e = Tolerance::new();
                    e.common = entity_common;
                    e.insertion_point = data.insertion_point;
                    e.text = data.text;
                    e.direction = data.direction;
                    e.dimension_style_handle = Some(Handle::from(data.dimstyle_handle));
                    let _ = document.add_entity(EntityType::Tolerance(e));
                },

                // ── Complex entities ───────────────────────────────
                OBJ_HATCH => {
                    let data = entities::read_hatch(&mut reader, self.obj_reader.version());
                    let mut e = Hatch::new();
                    e.common = entity_common;
                    e.elevation = data.elevation;
                    e.normal = data.normal;
                    e.pattern = HatchPattern::new(&data.pattern_name);
                    e.is_solid = data.is_solid;
                    e.is_associative = data.is_associative;
                    e.is_double = data.is_double;
                    e.pattern_angle = data.pattern_angle;
                    e.pattern_scale = data.pattern_scale;
                    // Convert DWG boundary paths to entity BoundaryPath
                    e.paths = data.paths.into_iter().map(|hp| {
                        use crate::entities::hatch::*;
                        let mut bp = BoundaryPath::with_flags(
                            BoundaryPathFlags::from_bits(hp.flags as u32),
                        );
                        // Polyline boundary path
                        if !hp.polyline_vertices.is_empty() {
                            let pe = PolylineEdge {
                                vertices: hp.polyline_vertices.iter()
                                    .map(|(pt, bulge)| crate::types::Vector3::new(pt.x, pt.y, *bulge))
                                    .collect(),
                                is_closed: hp.polyline_closed,
                            };
                            bp.add_edge(BoundaryEdge::Polyline(pe));
                        }
                        // Edge-type boundary path
                        for edge in hp.edges {
                            match edge {
                                crate::io::dwg::dwg_stream_readers::object_reader::entities::HatchEdge::Line(l) => {
                                    bp.add_edge(BoundaryEdge::Line(LineEdge {
                                        start: l.start,
                                        end: l.end,
                                    }));
                                }
                                crate::io::dwg::dwg_stream_readers::object_reader::entities::HatchEdge::Arc(a) => {
                                    bp.add_edge(BoundaryEdge::CircularArc(CircularArcEdge {
                                        center: a.center,
                                        radius: a.radius,
                                        start_angle: a.start_angle,
                                        end_angle: a.end_angle,
                                        counter_clockwise: a.ccw,
                                    }));
                                }
                                crate::io::dwg::dwg_stream_readers::object_reader::entities::HatchEdge::Ellipse(el) => {
                                    bp.add_edge(BoundaryEdge::EllipticArc(EllipticArcEdge {
                                        center: el.center,
                                        major_axis_endpoint: el.major_endpoint,
                                        minor_axis_ratio: el.minor_ratio,
                                        start_angle: el.start_angle,
                                        end_angle: el.end_angle,
                                        counter_clockwise: el.ccw,
                                    }));
                                }
                                crate::io::dwg::dwg_stream_readers::object_reader::entities::HatchEdge::Spline(s) => {
                                    bp.add_edge(BoundaryEdge::Spline(SplineEdge {
                                        degree: s.degree,
                                        rational: s.rational,
                                        periodic: s.periodic,
                                        knots: s.knots,
                                        control_points: s.control_points,
                                        fit_points: s.fit_points,
                                        start_tangent: s.start_tangent,
                                        end_tangent: s.end_tangent,
                                    }));
                                }
                            }
                        }
                        bp
                    }).collect();
                    e.seed_points = data.seed_points;
                    let _ = document.add_entity(EntityType::Hatch(e));
                },
                OBJ_VIEWPORT => {
                    let data = entities::read_viewport(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut e = Viewport::new();
                    e.common = entity_common;
                    e.center = data.center;
                    e.width = data.width;
                    e.height = data.height;
                    e.view_center = crate::types::Vector3::new(data.view_center.x, data.view_center.y, 0.0);
                    e.view_direction = data.view_direction;
                    e.view_target = data.view_target;
                    e.view_height = data.view_height;
                    e.lens_length = data.lens_length;
                    e.front_clip_z = data.front_clip_z;
                    e.back_clip_z = data.back_clip_z;
                    e.twist_angle = data.twist_angle;
                    let _ = document.add_entity(EntityType::Viewport(e));
                },
                OBJ_POLYLINE_2D => {
                    let data = entities::read_polyline2d(&mut reader, self.obj_reader.version());
                    let mut e = Polyline2D::new();
                    e.common = entity_common;
                    e.elevation = data.elevation;
                    e.thickness = data.thickness;
                    e.normal = data.normal;
                    e.start_width = data.start_width;
                    e.end_width = data.end_width;
                    let h = e.common.handle.value();
                    pending.polylines.push((h, EntityType::Polyline2D(e)));
                },
                OBJ_POLYLINE_3D => {
                    let data = entities::read_polyline3d(&mut reader, self.obj_reader.version());
                    let mut e = Polyline3D::new();
                    e.common = entity_common;
                    e.flags.closed = (data.closed_flag & 1) != 0;
                    let h = e.common.handle.value();
                    pending.polylines.push((h, EntityType::Polyline3D(e)));
                },

                // ── Dimension types ────────────────────────────────
                OBJ_DIMENSION_LINEAR => {
                    let data = entities::read_dimension_linear(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut dim = DimensionLinear::new(
                        data.first_point,
                        data.second_point,
                    );
                    dim.base.common = entity_common;
                    dim.base.text_middle_point = data.common.text_middle_point;
                    dim.base.normal = data.common.normal;
                    dim.rotation = data.rotation;
                    let _ = document.add_entity(EntityType::Dimension(
                        Dimension::Linear(dim),
                    ));
                },
                OBJ_DIMENSION_ALIGNED => {
                    let data = entities::read_dimension_aligned(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut dim = DimensionAligned::new(
                        data.first_point,
                        data.second_point,
                    );
                    dim.base.common = entity_common;
                    dim.base.text_middle_point = data.common.text_middle_point;
                    dim.base.normal = data.common.normal;
                    let _ = document.add_entity(EntityType::Dimension(
                        Dimension::Aligned(dim),
                    ));
                },
                OBJ_DIMENSION_RADIUS => {
                    let data = entities::read_dimension_radius(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut dim = DimensionRadius::new(
                        data.angle_vertex,
                        data.definition_point,
                    );
                    dim.base.common = entity_common;
                    dim.base.text_middle_point = data.common.text_middle_point;
                    dim.leader_length = data.leader_length;
                    let _ = document.add_entity(EntityType::Dimension(
                        Dimension::Radius(dim),
                    ));
                },
                OBJ_DIMENSION_DIAMETER => {
                    let data = entities::read_dimension_diameter(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut dim = DimensionDiameter::new(
                        data.angle_vertex,
                        data.definition_point,
                    );
                    dim.base.common = entity_common;
                    dim.base.text_middle_point = data.common.text_middle_point;
                    dim.leader_length = data.leader_length;
                    let _ = document.add_entity(EntityType::Dimension(
                        Dimension::Diameter(dim),
                    ));
                },
                OBJ_DIMENSION_ANG_2LN => {
                    let data = entities::read_dimension_angular_2ln(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut dim = DimensionAngular2Ln::default();
                    dim.base.common = entity_common;
                    dim.base.text_middle_point = data.common.text_middle_point;
                    dim.first_point = data.first_point;
                    dim.second_point = data.second_point;
                    dim.angle_vertex = data.angle_vertex;
                    dim.definition_point = data.definition_point;
                    let _ = document.add_entity(EntityType::Dimension(
                        Dimension::Angular2Ln(dim),
                    ));
                },
                OBJ_DIMENSION_ANG_3PT => {
                    let data = entities::read_dimension_angular_3pt(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut dim = DimensionAngular3Pt::default();
                    dim.base.common = entity_common;
                    dim.base.text_middle_point = data.common.text_middle_point;
                    dim.first_point = data.first_point;
                    dim.second_point = data.second_point;
                    dim.angle_vertex = data.angle_vertex;
                    dim.definition_point = data.definition_point;
                    let _ = document.add_entity(EntityType::Dimension(
                        Dimension::Angular3Pt(dim),
                    ));
                },
                OBJ_DIMENSION_ORDINATE => {
                    let data = entities::read_dimension_ordinate(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut dim = DimensionOrdinate::new(
                        data.feature_location,
                        data.leader_endpoint,
                        data.is_ordinate_type_x,
                    );
                    dim.base.common = entity_common;
                    dim.base.text_middle_point = data.common.text_middle_point;
                    let _ = document.add_entity(EntityType::Dimension(
                        Dimension::Ordinate(dim),
                    ));
                },

                OBJ_MLINE => {
                    let data = entities::read_mline(&mut reader);
                    let mut e = MLine::new();
                    e.common = entity_common;
                    e.scale_factor = data.scale_factor;
                    e.justification = MLineJustification::from(data.justification as i16);
                    e.start_point = data.start_point;
                    e.normal = data.normal;
                    e.style_element_count = data.lines_in_style as usize;
                    // Populate vertices from parsed data
                    e.vertices = data.vertices.into_iter().map(|vd| {
                        use crate::entities::mline::{MLineVertex, MLineSegment};
                        let mut mv = MLineVertex::new(vd.position);
                        mv.direction = vd.direction;
                        mv.miter = vd.miter;
                        mv.segments = vd.segments.into_iter().map(|sd| {
                            MLineSegment {
                                parameters: sd.parameters,
                                area_fill_parameters: sd.area_fill_parameters,
                            }
                        }).collect();
                        mv
                    }).collect();
                    let _ = document.add_entity(EntityType::MLine(e));
                },

                OBJ_POLYLINE_PFACE => {
                    let (_num_verts, _num_faces, _owned_count) = entities::read_polyface_mesh(
                        &mut reader, self.obj_reader.version(),
                    );
                    let mut e = PolyfaceMesh::new();
                    e.common = entity_common;
                    let h = e.common.handle.value();
                    pending.polylines.push((h, EntityType::PolyfaceMesh(e)));
                },

                OBJ_MESH => {
                    let data = entities::read_mesh(&mut reader);
                    let mut e = Mesh::new();
                    e.common = entity_common;
                    e.version = data.version;
                    e.blend_crease = data.blend_crease;
                    e.subdivision_level = data.subdivision_level;
                    e.vertices = data.vertices;
                    e.faces = data.faces.into_iter().map(|f| MeshFace { vertices: f.into_iter().map(|v| v as usize).collect() }).collect();
                    e.edges = data.edges.into_iter().map(|(a, b)| MeshEdge { start: a as usize, end: b as usize, crease: None }).collect();
                    let _ = document.add_entity(EntityType::Mesh(e));
                },

                OBJ_MULTILEADER => {
                    let data = entities::read_multileader(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut e = MultiLeader::new();
                    e.common = entity_common;
                    e.context = data.context;
                    e.style_handle = if data.style_handle != 0 { Some(Handle::from(data.style_handle)) } else { None };
                    e.property_override_flags = MultiLeaderPropertyOverrideFlags::from_bits_truncate(data.property_override_flags);
                    e.path_type = MultiLeaderPathType::from(data.path_type);
                    e.line_color = data.line_color;
                    e.line_type_handle = if data.line_type_handle != 0 { Some(Handle::from(data.line_type_handle)) } else { None };
                    e.line_weight = LineWeight::from_value(data.line_weight as i16);
                    e.enable_landing = data.enable_landing;
                    e.enable_dogleg = data.enable_dogleg;
                    e.dogleg_length = data.dogleg_length;
                    e.arrowhead_handle = if data.arrowhead_handle != 0 { Some(Handle::from(data.arrowhead_handle)) } else { None };
                    e.arrowhead_size = data.arrowhead_size;
                    e.content_type = LeaderContentType::from(data.content_type);
                    e.text_style_handle = if data.text_style_handle != 0 { Some(Handle::from(data.text_style_handle)) } else { None };
                    e.text_left_attachment = TextAttachmentType::from(data.text_left_attachment);
                    e.text_right_attachment = TextAttachmentType::from(data.text_right_attachment);
                    e.text_angle_type = TextAngleType::from(data.text_angle_type);
                    e.text_alignment = TextAlignmentType::from(data.text_alignment);
                    e.text_color = data.text_color;
                    e.text_frame = data.text_frame;
                    e.block_content_handle = if data.block_content_handle != 0 { Some(Handle::from(data.block_content_handle)) } else { None };
                    e.block_content_color = data.block_content_color;
                    e.block_scale = data.block_scale;
                    e.block_rotation = data.block_rotation;
                    e.block_connection_type = BlockContentConnectionType::from(data.block_connection_type);
                    e.enable_annotation_scale = data.enable_annotation_scale;
                    e.block_attributes = data.block_attributes;
                    e.text_direction_negative = data.text_direction_negative;
                    e.text_align_in_ipe = data.text_align_in_ipe;
                    e.text_attachment_point = TextAttachmentPointType::from(data.text_attachment_point);
                    e.scale_factor = data.scale_factor;
                    e.text_attachment_direction = TextAttachmentDirectionType::from(data.text_attachment_direction);
                    e.text_bottom_attachment = TextAttachmentType::from(data.text_bottom_attachment);
                    e.text_top_attachment = TextAttachmentType::from(data.text_top_attachment);
                    e.extend_leader_to_text = data.extend_leader_to_text;
                    let _ = document.add_entity(EntityType::MultiLeader(e));
                },

                // ── Attribute entities ─────────────────────────────
                OBJ_ATTDEF => {
                    let data = entities::read_attribute_definition(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut e = AttributeDefinition::new(
                        data.tag.clone(),
                        String::new(), // prompt (consumed by reader, not returned separately)
                        data.text_data.value.clone(),
                    );
                    e.common = entity_common;
                    e.insertion_point = data.text_data.insertion_point;
                    e.height = data.text_data.height;
                    e.rotation = data.text_data.rotation;
                    let _ = document.add_entity(EntityType::AttributeDefinition(e));
                },
                OBJ_ATTRIB => {
                    let data = entities::read_attribute_entity(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut e = AttributeEntity::new(
                        data.tag.clone(),
                        data.text_data.value.clone(),
                    );
                    e.common = entity_common;
                    e.insertion_point = data.text_data.insertion_point;
                    e.height = data.text_data.height;
                    e.rotation = data.text_data.rotation;
                    let _ = document.add_entity(EntityType::AttributeEntity(e));
                },

                // ── Structural markers (BLOCK / ENDBLK / SEQEND) ──
                // These are DWG-internal structural entities. They mark
                // block boundaries and sequence terminators. They are
                // silently consumed — their information is already
                // represented by BlockRecord table entries.
                OBJ_BLOCK => {
                    // BLOCK entity has no entity-specific data beyond common.
                    // The block name comes from the BlockRecord (Pass 1).
                    // We intentionally do NOT add it as an entity.
                },
                OBJ_ENDBLK => {
                    // ENDBLK marks the end of a block definition.
                    // Silently skip.
                },
                OBJ_SEQEND => {
                    // SEQEND terminates a polyline vertex or INSERT
                    // attribute sequence. Silently skip.
                    entities::read_seqend(&mut reader);
                },

                // ── Vertex child entities ──────────────────────────
                // Vertex records are children of POLYLINE_2D,
                // POLYLINE_3D, POLYLINE_PFACE, or POLYLINE_MESH.
                // Collect vertex data and attach to parent polylines
                // in the post-processing step after Pass 2.
                OBJ_VERTEX_2D => {
                    let data = entities::read_vertex2d(
                        &mut reader, self.obj_reader.version(),
                    );
                    pending.vertices.entry(entity_data.owner_handle)
                        .or_default()
                        .push(PendingVertex::V2D(data));
                },
                OBJ_VERTEX_3D | OBJ_VERTEX_MESH => {
                    let data = entities::read_vertex3d(&mut reader);
                    pending.vertices.entry(entity_data.owner_handle)
                        .or_default()
                        .push(PendingVertex::V3D(data));
                },
                OBJ_VERTEX_PFACE => {
                    let data = entities::read_vertex3d(&mut reader);
                    pending.vertices.entry(entity_data.owner_handle)
                        .or_default()
                        .push(PendingVertex::V3D(data));
                },
                OBJ_VERTEX_PFACE_FACE => {
                    let data = entities::read_pface_face(&mut reader);
                    pending.vertices.entry(entity_data.owner_handle)
                        .or_default()
                        .push(PendingVertex::PfaceFace(data));
                },

                // ── Catch-all ──────────────────────────────────────
                _ => {
                    let mut e = UnknownEntity::new(format!("DWG_TYPE_{}", type_code));
                    e.common = entity_common;
                    let _ = document.add_entity(EntityType::Unknown(e));
                }
            }
        } else if !is_table_type(type_code) {
            // ── Non-graphical objects ──────────────────────────────
            let _non_entity_data = self.obj_reader.read_common_non_entity_data(&mut reader, type_code);

            match type_code {
                OBJ_DICTIONARY => {
                    let data = objects::read_dictionary(&mut reader, self.obj_reader.version());
                    let mut obj = crate::objects::Dictionary::new();
                    obj.handle = Handle::from(handle);
                    obj.hard_owner = data.hard_owner;
                    obj.duplicate_cloning = data.duplicate_cloning;
                    for entry in data.entries {
                        obj.add_entry(entry.name, Handle::from(entry.handle));
                    }
                    document.objects.insert(
                        Handle::from(handle),
                        crate::objects::ObjectType::Dictionary(obj),
                    );
                },
                OBJ_DICTIONARYWDFLT => {
                    let data = objects::read_dictionary_with_default(&mut reader, self.obj_reader.version());
                    let mut obj = crate::objects::DictionaryWithDefault::new();
                    obj.handle = Handle::from(handle);
                    document.objects.insert(
                        Handle::from(handle),
                        crate::objects::ObjectType::DictionaryWithDefault(obj),
                    );
                    // Also store entries in a regular dict for lookup
                    let mut dict = crate::objects::Dictionary::new();
                    dict.handle = Handle::from(handle);
                    dict.hard_owner = data.hard_owner;
                    dict.duplicate_cloning = data.duplicate_cloning;
                    for entry in data.entries {
                        dict.add_entry(entry.name, Handle::from(entry.handle));
                    }
                },
                OBJ_DICTIONARYVAR => {
                    let data = objects::read_dictionary_variable(&mut reader);
                    let mut obj = crate::objects::DictionaryVariable::new("", &data.value);
                    obj.handle = Handle::from(handle);
                    obj.schema_number = data.schema_number as i16;
                    document.objects.insert(
                        Handle::from(handle),
                        crate::objects::ObjectType::DictionaryVariable(obj),
                    );
                },
                OBJ_LAYOUT => {
                    let data = objects::read_layout(&mut reader, self.obj_reader.version());
                    let mut obj = crate::objects::Layout::new(&data.name);
                    obj.handle = Handle::from(handle);
                    obj.flags = data.flags;
                    obj.tab_order = data.tab_order as i16;
                    obj.min_limits = data.min_limits;
                    obj.max_limits = data.max_limits;
                    obj.insertion_base = (
                        data.insertion_base.x,
                        data.insertion_base.y,
                        data.insertion_base.z,
                    );
                    obj.min_extents = (
                        data.min_extents.x,
                        data.min_extents.y,
                        data.min_extents.z,
                    );
                    obj.max_extents = (
                        data.max_extents.x,
                        data.max_extents.y,
                        data.max_extents.z,
                    );
                    obj.block_record = Handle::from(data.block_record_handle);
                    obj.viewport = Handle::from(data.viewport_handle);
                    document.objects.insert(
                        Handle::from(handle),
                        crate::objects::ObjectType::Layout(obj),
                    );
                },
                OBJ_GROUP => {
                    let data = objects::read_group(&mut reader);
                    let mut obj = crate::objects::Group::new("");
                    obj.description = data.description;
                    obj.selectable = data.selectable;
                    for eh in data.entity_handles {
                        obj.entities.push(Handle::from(eh));
                    }
                    document.objects.insert(
                        Handle::from(handle),
                        crate::objects::ObjectType::Group(obj),
                    );
                },
                OBJ_MLINESTYLE => {
                    let data = objects::read_mlinestyle(&mut reader, self.obj_reader.version(), self.obj_reader.dxf_version());
                    let obj = crate::objects::MLineStyle::new(&data.name);
                    document.objects.insert(
                        Handle::from(handle),
                        crate::objects::ObjectType::MLineStyle(obj),
                    );
                },
                OBJ_XRECORD => {
                    let _data = objects::read_xrecord(&mut reader);
                    let mut obj = crate::objects::XRecord::new();
                    obj.handle = Handle::from(handle);
                    document.objects.insert(
                        Handle::from(handle),
                        crate::objects::ObjectType::XRecord(obj),
                    );
                },
                _ => {
                    // Skip unsupported object types
                }
            }
        }
        // Table types already processed in Pass 1
    }

    /// Build a class_number → internal OBJ_* type code mapping.
    ///
    /// The DWG binary uses class numbers (≥500) for non-fixed object types.
    /// This builds a translation table so the builder can match them against
    /// the internal OBJ_* constants.
    fn build_class_type_map(document: &CadDocument) -> HashMap<i16, i16> {
        let mut map = HashMap::new();
        for class in document.classes.iter() {
            if let Some(internal_code) = dxf_name_to_type_code(&class.dxf_name) {
                if class.class_number >= 500 {
                    map.insert(class.class_number, internal_code);
                }
            }
        }
        map
    }

    /// Resolve a raw DWG type code to the internal OBJ_* constant.
    ///
    /// Fixed type codes (0–82) pass through unchanged.
    /// Class-based codes (≥500) are looked up in the class map.
    fn resolve_type_code(raw: i16, class_map: &HashMap<i16, i16>) -> i16 {
        if raw >= 500 {
            class_map.get(&raw).copied().unwrap_or(raw)
        } else {
            raw
        }
    }
}

fn map_entity_common(data: &EntityCommonData, maps: &HandleMaps) -> EntityCommon {
    let mut common = EntityCommon::new();
    common.handle = Handle::from(data.common.handle);
    common.owner_handle = Handle::from(data.owner_handle);
    common.color = data.color;
    common.transparency = data.transparency;
    common.invisible = data.invisible;
    common.layer = maps.layer_name(data.layer_handle);
    common
}
