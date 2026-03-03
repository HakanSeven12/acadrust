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
use crate::error::Result;
use crate::types::Handle;
use crate::io::dwg::dwg_stream_readers::object_reader::{
    DwgObjectReader, EntityCommonData,
};
use crate::io::dwg::dwg_stream_readers::object_reader::common::*;
use crate::io::dwg::dwg_stream_readers::object_reader::entities;
use crate::io::dwg::dwg_stream_readers::object_reader::objects;
use crate::io::dwg::dwg_stream_readers::object_reader::tables;

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

    #[allow(dead_code)]
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
}

impl DwgDocumentBuilder {
    /// Create a new builder wrapping the object reader.
    pub fn new(obj_reader: DwgObjectReader) -> Self {
        Self { obj_reader }
    }

    /// Build the document by iterating all handles and dispatching objects.
    ///
    /// Uses a two-pass approach:
    /// 1. Read table entries → build handle→name maps
    /// 2. Read entities and objects → resolve handle references
    pub fn build(self, document: &mut CadDocument) -> Result<()> {
        let handles = self.obj_reader.handles();

        // ── Pass 1: Build handle→name maps from table entries ──────────
        let mut maps = HandleMaps::new();

        for &handle in &handles {
            let offset = match self.obj_reader.offset_for(handle) {
                Some(o) if o >= 0 => o,
                _ => continue,
            };
            let (type_code, mut reader) = match self.obj_reader.read_record_at(offset as usize) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if is_table_type(type_code) {
                // Wrap in catch_unwind to survive corrupt/misaligned records
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let non_entity = self.obj_reader.read_common_non_entity_data(&mut reader, type_code);
                    let obj_handle = non_entity.common.handle;
                    (obj_handle, type_code)
                }));
                let (obj_handle, type_code) = match result {
                    Ok(v) => v,
                    Err(_) => continue,  // skip corrupt record
                };

                let table_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match type_code {
                        OBJ_LAYER => {
                            let data = tables::read_layer(
                                &mut reader,
                                self.obj_reader.version(),
                                self.obj_reader.dxf_version(),
                            );
                            Some(("layer", obj_handle, data.name))
                        },
                        OBJ_BLOCK_HEADER => {
                            let data = tables::read_block_header(
                                &mut reader,
                                self.obj_reader.version(),
                            );
                            Some(("block", obj_handle, data.name))
                        },
                        OBJ_STYLE => {
                            let data = tables::read_text_style(&mut reader);
                            Some(("style", obj_handle, data.name))
                        },
                        OBJ_LTYPE => {
                            let data = tables::read_linetype(
                                &mut reader,
                                self.obj_reader.version(),
                            );
                            Some(("ltype", obj_handle, data.name))
                        },
                        OBJ_DIMSTYLE => {
                            let data = tables::read_dimstyle(
                                &mut reader,
                                self.obj_reader.version(),
                                self.obj_reader.dxf_version(),
                            );
                            Some(("dimstyle", obj_handle, data.name))
                        },
                        _ => None,
                    }
                }));
                if let Ok(Some((kind, h, name))) = table_result {
                    match kind {
                        "layer" => { maps.layers.insert(h, name); },
                        "block" => { maps.blocks.insert(h, name); },
                        "style" => { maps.text_styles.insert(h, name); },
                        "ltype" => { maps.linetypes.insert(h, name); },
                        "dimstyle" => { maps.dim_styles.insert(h, name); },
                        _ => {}
                    }
                }
            }
        }

        // ── Pass 2: Read entities and non-table objects ────────────────
        for &handle in &handles {
            let offset = match self.obj_reader.offset_for(handle) {
                Some(o) if o >= 0 => o,
                _ => continue,
            };
            let (type_code, reader) = match self.obj_reader.read_record_at(offset as usize) {
                Ok(r) => r,
                Err(_) => continue,
            };

            // Wrap per-object processing in catch_unwind to survive
            // corrupt or misaligned records without crashing the entire read.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.process_pass2_record(handle, type_code, reader, document, &maps);
            }));
            if result.is_err() {
                // Silently skip corrupt object — it will be missing from the document
                continue;
            }
        }

        Ok(())
    }

    /// Process a single object record in Pass 2.
    fn process_pass2_record(
        &self,
        handle: u64,
        type_code: i16,
        mut reader: crate::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader,
        document: &mut CadDocument,
        maps: &HandleMaps,
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
                    let _ = document.add_entity(EntityType::LwPolyline(e));
                },
                OBJ_SPLINE => {
                    let data = entities::read_spline(
                        &mut reader, self.obj_reader.version(), self.obj_reader.dxf_version(),
                    );
                    let mut e = Spline::new();
                    e.common = entity_common;
                    e.degree = data.degree;
                    e.knots = data.knots;
                    e.control_points = data.control_points;
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
                    e.alignment_point = Some(data.alignment_point);
                    e.rotation = data.rotation;
                    e.oblique_angle = data.oblique_angle;
                    e.width_factor = data.width_factor;
                    e.normal = data.normal;
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
                    let _ = document.add_entity(EntityType::Polyline2D(e));
                },
                OBJ_POLYLINE_3D => {
                    let _data = entities::read_polyline3d(&mut reader, self.obj_reader.version());
                    let mut e = Polyline3D::new();
                    e.common = entity_common;
                    let _ = document.add_entity(EntityType::Polyline3D(e));
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
