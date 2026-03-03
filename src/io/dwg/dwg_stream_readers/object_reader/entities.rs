//! Entity readers for DWG object section.
//!
//! Each reader is the exact inverse of the corresponding writer in
//! `dwg_stream_writers/object_writer/entities.rs`. They read entity-specific
//! fields after common entity data has already been parsed.

use crate::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use crate::io::dwg::dwg_version::DwgVersion;
use crate::types::{Vector2, Vector3, DxfVersion};
use super::safe_count;

// ════════════════════════════════════════════════════════════════════════
//  Result structs
// ════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct PointData {
    pub location: Vector3,
    pub thickness: f64,
    pub normal: Vector3,
    pub x_axis_angle: f64,
}

#[derive(Debug, Clone)]
pub struct LineData {
    pub start: Vector3,
    pub end: Vector3,
    pub thickness: f64,
    pub normal: Vector3,
}

#[derive(Debug, Clone)]
pub struct CircleData {
    pub center: Vector3,
    pub radius: f64,
    pub thickness: f64,
    pub normal: Vector3,
}

#[derive(Debug, Clone)]
pub struct ArcData {
    pub center: Vector3,
    pub radius: f64,
    pub thickness: f64,
    pub normal: Vector3,
    pub start_angle: f64,
    pub end_angle: f64,
}

#[derive(Debug, Clone)]
pub struct EllipseData {
    pub center: Vector3,
    pub major_axis: Vector3,
    pub normal: Vector3,
    pub minor_axis_ratio: f64,
    pub start_parameter: f64,
    pub end_parameter: f64,
}

#[derive(Debug, Clone)]
pub struct RayData {
    pub base_point: Vector3,
    pub direction: Vector3,
}

#[derive(Debug, Clone)]
pub struct XLineData {
    pub base_point: Vector3,
    pub direction: Vector3,
}

#[derive(Debug, Clone)]
pub struct SolidData {
    pub thickness: f64,
    pub elevation: f64,
    pub first_corner: Vector2,
    pub second_corner: Vector2,
    pub third_corner: Vector2,
    pub fourth_corner: Vector2,
    pub normal: Vector3,
}

#[derive(Debug, Clone)]
pub struct Face3DData {
    pub first_corner: Vector3,
    pub second_corner: Vector3,
    pub third_corner: Vector3,
    pub fourth_corner: Vector3,
    pub invisible_edges: i16,
}

#[derive(Debug, Clone)]
pub struct InsertData {
    pub insert_point: Vector3,
    pub x_scale: f64,
    pub y_scale: f64,
    pub z_scale: f64,
    pub rotation: f64,
    pub normal: Vector3,
    pub has_attribs: bool,
    pub block_handle: u64,
}

#[derive(Debug, Clone)]
pub struct LwPolylineVertex {
    pub x: f64,
    pub y: f64,
    pub bulge: f64,
    pub start_width: f64,
    pub end_width: f64,
}

#[derive(Debug, Clone)]
pub struct LwPolylineData {
    pub flag: i16,
    pub constant_width: f64,
    pub elevation: f64,
    pub thickness: f64,
    pub normal: Vector3,
    pub vertices: Vec<LwPolylineVertex>,
}

#[derive(Debug, Clone)]
pub struct SplineData {
    pub scenario: i32,
    pub degree: i32,
    pub rational: bool,
    pub closed: bool,
    pub periodic: bool,
    pub knot_tolerance: f64,
    pub control_tolerance: f64,
    pub knots: Vec<f64>,
    pub control_points: Vec<Vector3>,
    pub weights: Vec<f64>,
    pub fit_tolerance: f64,
    pub begin_tangent: Vector3,
    pub end_tangent: Vector3,
    pub fit_points: Vec<Vector3>,
}

#[derive(Debug, Clone)]
pub struct TextEntityData {
    pub insertion_point: Vector3,
    pub alignment_point: Vector3,
    pub normal: Vector3,
    pub thickness: f64,
    pub oblique_angle: f64,
    pub rotation: f64,
    pub height: f64,
    pub width_factor: f64,
    pub value: String,
    pub generation: i16,
    pub horizontal_alignment: i16,
    pub vertical_alignment: i16,
    pub style_handle: u64,
}

#[derive(Debug, Clone)]
pub struct MTextData {
    pub insertion_point: Vector3,
    pub normal: Vector3,
    pub x_direction: Vector3,
    pub rectangle_width: f64,
    pub rectangle_height: f64,
    pub height: f64,
    pub attachment_point: i16,
    pub drawing_direction: i16,
    pub extents_height: f64,
    pub extents_width: f64,
    pub value: String,
    pub style_handle: u64,
    pub linespacing_style: i16,
    pub linespacing_factor: f64,
    pub unknown_bit: bool,
    pub background_flags: i32,
}

#[derive(Debug, Clone)]
pub struct ShapeData {
    pub insertion_point: Vector3,
    pub size: f64,
    pub rotation: f64,
    pub relative_x_scale: f64,
    pub oblique_angle: f64,
    pub thickness: f64,
    pub shape_number: i16,
    pub normal: Vector3,
    pub style_handle: u64,
}

#[derive(Debug, Clone)]
pub struct LeaderData {
    pub unknown_bit: bool,
    pub annotation_type: i16,
    pub path_type: i16,
    pub vertices: Vec<Vector3>,
    pub origin: Vector3,
    pub normal: Vector3,
    pub horizontal_direction: Vector3,
    pub block_offset: Vector3,
    pub annotation_offset: Vector3,
    pub text_height: f64,
    pub text_width: f64,
    pub hookline_on_x_dir: bool,
    pub arrowhead_on: bool,
    pub annotation_handle: u64,
    pub dimstyle_handle: u64,
}

#[derive(Debug, Clone)]
pub struct ToleranceData {
    pub insertion_point: Vector3,
    pub direction: Vector3,
    pub normal: Vector3,
    pub text: String,
    pub dimstyle_handle: u64,
}

// ════════════════════════════════════════════════════════════════════════
//  Reader functions — Simple entities
// ════════════════════════════════════════════════════════════════════════

pub fn read_point(reader: &mut DwgMergedReader) -> PointData {
    let location = reader.read_3bit_double();
    let thickness = reader.read_bit_thickness();
    let normal = reader.read_bit_extrusion();
    let x_axis_angle = reader.read_bit_double();
    PointData { location, thickness, normal, x_axis_angle }
}

pub fn read_line(reader: &mut DwgMergedReader, version: DwgVersion) -> LineData {
    let (start, end);
    if version.r13_14_only() {
        start = reader.read_3bit_double();
        end = reader.read_3bit_double();
    } else {
        let z_are_zero = reader.read_bit();
        let sx = reader.read_raw_double();
        let ex = reader.read_bit_double_with_default(sx);
        let sy = reader.read_raw_double();
        let ey = reader.read_bit_double_with_default(sy);
        let (sz, ez) = if !z_are_zero {
            let sz = reader.read_raw_double();
            let ez = reader.read_bit_double_with_default(sz);
            (sz, ez)
        } else {
            (0.0, 0.0)
        };
        start = Vector3::new(sx, sy, sz);
        end = Vector3::new(ex, ey, ez);
    }
    let thickness = reader.read_bit_thickness();
    let normal = reader.read_bit_extrusion();
    LineData { start, end, thickness, normal }
}

pub fn read_circle(reader: &mut DwgMergedReader) -> CircleData {
    let center = reader.read_3bit_double();
    let radius = reader.read_bit_double();
    let thickness = reader.read_bit_thickness();
    let normal = reader.read_bit_extrusion();
    CircleData { center, radius, thickness, normal }
}

pub fn read_arc(reader: &mut DwgMergedReader) -> ArcData {
    let center = reader.read_3bit_double();
    let radius = reader.read_bit_double();
    let thickness = reader.read_bit_thickness();
    let normal = reader.read_bit_extrusion();
    let start_angle = reader.read_bit_double();
    let end_angle = reader.read_bit_double();
    ArcData { center, radius, thickness, normal, start_angle, end_angle }
}

pub fn read_ellipse(reader: &mut DwgMergedReader) -> EllipseData {
    let center = reader.read_3bit_double();
    let major_axis = reader.read_3bit_double();
    let normal = reader.read_3bit_double();
    let minor_axis_ratio = reader.read_bit_double();
    let start_parameter = reader.read_bit_double();
    let end_parameter = reader.read_bit_double();
    EllipseData { center, major_axis, normal, minor_axis_ratio, start_parameter, end_parameter }
}

pub fn read_ray(reader: &mut DwgMergedReader) -> RayData {
    let base_point = reader.read_3bit_double();
    let direction = reader.read_3bit_double();
    RayData { base_point, direction }
}

pub fn read_xline(reader: &mut DwgMergedReader) -> XLineData {
    let base_point = reader.read_3bit_double();
    let direction = reader.read_3bit_double();
    XLineData { base_point, direction }
}

pub fn read_solid(reader: &mut DwgMergedReader) -> SolidData {
    let thickness = reader.read_bit_thickness();
    let elevation = reader.read_bit_double();
    let first_corner = reader.read_2raw_double();
    let second_corner = reader.read_2raw_double();
    let third_corner = reader.read_2raw_double();
    let fourth_corner = reader.read_2raw_double();
    let normal = reader.read_bit_extrusion();
    SolidData { thickness, elevation, first_corner, second_corner, third_corner, fourth_corner, normal }
}

pub fn read_face3d(reader: &mut DwgMergedReader, version: DwgVersion) -> Face3DData {
    if version.r13_14_only() {
        let first_corner = reader.read_3bit_double();
        let second_corner = reader.read_3bit_double();
        let third_corner = reader.read_3bit_double();
        let fourth_corner = reader.read_3bit_double();
        let invisible_edges = reader.read_bit_short();
        Face3DData { first_corner, second_corner, third_corner, fourth_corner, invisible_edges }
    } else {
        let has_no_flags = reader.read_bit();
        let z_are_same = reader.read_bit();

        let x1 = reader.read_raw_double();
        let y1 = reader.read_raw_double();
        let z1 = if !z_are_same { reader.read_raw_double() } else { 0.0 };

        let x2 = reader.read_bit_double_with_default(x1);
        let y2 = reader.read_bit_double_with_default(y1);
        let z2 = if !z_are_same { reader.read_bit_double_with_default(z1) } else { z1 };

        let x3 = reader.read_bit_double_with_default(x2);
        let y3 = reader.read_bit_double_with_default(y2);
        let z3 = if !z_are_same { reader.read_bit_double_with_default(z2) } else { z1 };

        let x4 = reader.read_bit_double_with_default(x3);
        let y4 = reader.read_bit_double_with_default(y3);
        let z4 = if !z_are_same { reader.read_bit_double_with_default(z3) } else { z1 };

        let invisible_edges = if !has_no_flags { reader.read_bit_short() } else { 0 };

        Face3DData {
            first_corner: Vector3::new(x1, y1, z1),
            second_corner: Vector3::new(x2, y2, z2),
            third_corner: Vector3::new(x3, y3, z3),
            fourth_corner: Vector3::new(x4, y4, z4),
            invisible_edges,
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
//  Reader functions — Moderate entities
// ════════════════════════════════════════════════════════════════════════

pub fn read_insert(reader: &mut DwgMergedReader, version: DwgVersion) -> InsertData {
    let insert_point = reader.read_3bit_double();
    let (x_scale, y_scale, z_scale);

    if version.r13_14_only() {
        x_scale = reader.read_bit_double();
        y_scale = reader.read_bit_double();
        z_scale = reader.read_bit_double();
    } else {
        // R2000+
        let data_flags = reader.main_mut().read_2bits();
        match data_flags {
            3 => { x_scale = 1.0; y_scale = 1.0; z_scale = 1.0; }
            2 => {
                x_scale = reader.read_raw_double();
                y_scale = x_scale; z_scale = x_scale;
            }
            1 => {
                x_scale = 1.0;
                y_scale = reader.read_bit_double_with_default(1.0);
                z_scale = reader.read_bit_double_with_default(1.0);
            }
            _ => {
                x_scale = reader.read_raw_double();
                y_scale = reader.read_bit_double_with_default(x_scale);
                z_scale = reader.read_bit_double_with_default(x_scale);
            }
        }
    }

    let rotation = reader.read_bit_double();
    let normal = reader.read_3bit_double();
    let has_attribs = reader.read_bit();
    let block_handle = reader.read_handle();

    InsertData { insert_point, x_scale, y_scale, z_scale, rotation, normal, has_attribs, block_handle }
}

pub fn read_lwpolyline(reader: &mut DwgMergedReader, version: DwgVersion) -> LwPolylineData {
    let flag = reader.read_bit_short();
    let has_constant_width = (flag & 0x4) != 0;
    let has_elevation = (flag & 0x8) != 0;
    let has_thickness = (flag & 0x2) != 0;
    let has_normal = (flag & 0x1) != 0;
    let has_bulges = (flag & 0x10) != 0;
    let has_widths = (flag & 0x20) != 0;

    let constant_width = if has_constant_width { reader.read_bit_double() } else { 0.0 };
    let elevation = if has_elevation { reader.read_bit_double() } else { 0.0 };
    let thickness = if has_thickness { reader.read_bit_double() } else { 0.0 };
    let normal = if has_normal { reader.read_3bit_double() } else { Vector3::UNIT_Z };

    let num_pts = safe_count(reader.read_bit_long());
    let num_bulges = if has_bulges { safe_count(reader.read_bit_long()) } else { 0 };
    let num_widths = if has_widths { safe_count(reader.read_bit_long()) } else { 0 };

    // Read vertex positions
    let mut xs = Vec::with_capacity(num_pts as usize);
    let mut ys = Vec::with_capacity(num_pts as usize);

    if version.r13_14_only() {
        for _ in 0..num_pts {
            xs.push(reader.read_raw_double());
            ys.push(reader.read_raw_double());
        }
    } else if num_pts > 0 {
        // R2000+: first vertex is 2RD, rest are 2DD
        xs.push(reader.read_raw_double());
        ys.push(reader.read_raw_double());
        for i in 1..num_pts as usize {
            let px = xs[i - 1];
            let py = ys[i - 1];
            xs.push(reader.read_bit_double_with_default(px));
            ys.push(reader.read_bit_double_with_default(py));
        }
    }

    // Read bulges
    let mut bulges = vec![0.0f64; num_pts as usize];
    if has_bulges {
        for i in 0..num_bulges as usize {
            if i < bulges.len() {
                bulges[i] = reader.read_bit_double();
            }
        }
    }

    // Read widths
    let mut start_widths = vec![0.0f64; num_pts as usize];
    let mut end_widths = vec![0.0f64; num_pts as usize];
    if has_widths {
        for i in 0..num_widths as usize {
            if i < start_widths.len() {
                start_widths[i] = reader.read_bit_double();
                end_widths[i] = reader.read_bit_double();
            }
        }
    }

    let mut vertices = Vec::with_capacity(num_pts as usize);
    for i in 0..num_pts as usize {
        vertices.push(LwPolylineVertex {
            x: xs[i], y: ys[i],
            bulge: bulges[i],
            start_width: start_widths[i],
            end_width: end_widths[i],
        });
    }

    LwPolylineData { flag, constant_width, elevation, thickness, normal, vertices }
}

pub fn read_spline(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
    dxf_version: DxfVersion,
) -> SplineData {
    let mut _flags1 = 0i32;
    let mut _knot_param = 0i32;

    let scenario;
    if version.r2013_plus(dxf_version) {
        scenario = reader.read_bit_long();
        _flags1 = reader.read_bit_long();
        _knot_param = reader.read_bit_long();
    } else {
        scenario = reader.read_bit_long();
    }

    let degree = reader.read_bit_long();

    let mut rational = false;
    let mut closed = false;
    let mut periodic = false;
    let mut knot_tolerance = 0.0;
    let mut control_tolerance = 0.0;
    let mut knots = Vec::new();
    let mut control_points = Vec::new();
    let mut weights = Vec::new();
    let mut fit_tolerance = 0.0;
    let mut begin_tangent = Vector3::ZERO;
    let mut end_tangent = Vector3::ZERO;
    let mut fit_points = Vec::new();

    match scenario {
        1 => {
            rational = reader.read_bit();
            closed = reader.read_bit();
            periodic = reader.read_bit();
            knot_tolerance = reader.read_bit_double();
            control_tolerance = reader.read_bit_double();
            let num_knots = safe_count(reader.read_bit_long());
            let num_ctrl = safe_count(reader.read_bit_long());
            let has_weights = reader.read_bit();

            for _ in 0..num_knots {
                knots.push(reader.read_bit_double());
            }
            for _ in 0..num_ctrl {
                let pt = reader.read_3bit_double();
                control_points.push(pt);
                if has_weights {
                    weights.push(reader.read_bit_double());
                }
            }
        }
        _ => {
            fit_tolerance = reader.read_bit_double();
            begin_tangent = reader.read_3bit_double();
            end_tangent = reader.read_3bit_double();
            let num_fit = safe_count(reader.read_bit_long());
            for _ in 0..num_fit {
                fit_points.push(reader.read_3bit_double());
            }
        }
    }

    SplineData {
        scenario, degree, rational, closed, periodic,
        knot_tolerance, control_tolerance,
        knots, control_points, weights,
        fit_tolerance, begin_tangent, end_tangent, fit_points,
    }
}

/// Shared text entity data reader (used by Text, AttDef, AttEntity).
pub fn read_text_entity_data(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
) -> TextEntityData {
    if version.r13_14_only() {
        let elevation = reader.read_bit_double();
        let ix = reader.read_raw_double();
        let iy = reader.read_raw_double();
        let ax = reader.read_raw_double();
        let ay = reader.read_raw_double();
        let normal = reader.read_3bit_double();
        let thickness = reader.read_bit_double();
        let oblique_angle = reader.read_bit_double();
        let rotation = reader.read_bit_double();
        let height = reader.read_bit_double();
        let width_factor = reader.read_bit_double();
        let value = reader.read_variable_text();
        let generation = reader.read_bit_short();
        let horizontal_alignment = reader.read_bit_short();
        let vertical_alignment = reader.read_bit_short();

        TextEntityData {
            insertion_point: Vector3::new(ix, iy, elevation),
            alignment_point: Vector3::new(ax, ay, elevation),
            normal, thickness, oblique_angle, rotation,
            height, width_factor, value, generation,
            horizontal_alignment, vertical_alignment,
            style_handle: 0,
        }
    } else {
        let data_flags = reader.read_byte();
        let elevation = if (data_flags & 0x01) == 0 { reader.read_raw_double() } else { 0.0 };
        let ix = reader.read_raw_double();
        let iy = reader.read_raw_double();
        let (ax, ay) = if (data_flags & 0x02) == 0 {
            (reader.read_bit_double_with_default(ix),
             reader.read_bit_double_with_default(iy))
        } else { (0.0, 0.0) };
        let normal = reader.read_bit_extrusion();
        let thickness = reader.read_bit_thickness();
        let oblique_angle = if (data_flags & 0x04) == 0 { reader.read_raw_double() } else { 0.0 };
        let rotation = if (data_flags & 0x08) == 0 { reader.read_raw_double() } else { 0.0 };
        let height = reader.read_raw_double();
        let width_factor = if (data_flags & 0x10) == 0 { reader.read_raw_double() } else { 1.0 };
        let value = reader.read_variable_text();
        let generation = if (data_flags & 0x20) == 0 { reader.read_bit_short() } else { 0 };
        let horizontal_alignment = if (data_flags & 0x40) == 0 { reader.read_bit_short() } else { 0 };
        let vertical_alignment = if (data_flags & 0x80) == 0 { reader.read_bit_short() } else { 0 };

        TextEntityData {
            insertion_point: Vector3::new(ix, iy, elevation),
            alignment_point: Vector3::new(ax, ay, elevation),
            normal, thickness, oblique_angle, rotation,
            height, width_factor, value, generation,
            horizontal_alignment, vertical_alignment,
            style_handle: 0,
        }
    }
}

/// Read TEXT entity (wraps read_text_entity_data + style handle).
pub fn read_text(reader: &mut DwgMergedReader, version: DwgVersion) -> TextEntityData {
    let mut data = read_text_entity_data(reader, version);
    data.style_handle = reader.read_handle();
    data
}

pub fn read_mtext(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
    dxf_version: DxfVersion,
) -> MTextData {
    let insertion_point = reader.read_3bit_double();
    let normal = reader.read_3bit_double();
    let x_direction = reader.read_3bit_double();
    let rectangle_width = reader.read_bit_double();
    let rectangle_height = if version.r2007_plus() { reader.read_bit_double() } else { 0.0 };
    let height = reader.read_bit_double();
    let attachment_point = reader.read_bit_short();
    let drawing_direction = reader.read_bit_short();
    let extents_height = reader.read_bit_double();
    let extents_width = reader.read_bit_double();
    let value = reader.read_variable_text();

    let style_handle = reader.read_handle();

    let mut linespacing_style = 0i16;
    let mut linespacing_factor = 1.0;
    let mut unknown_bit = false;
    if version.r2000_plus() {
        linespacing_style = reader.read_bit_short();
        linespacing_factor = reader.read_bit_double();
        unknown_bit = reader.read_bit();
    }

    let mut background_flags = 0i32;
    if version.r2004_plus() {
        background_flags = reader.read_bit_long();
    }

    if version.r2018_plus(dxf_version) {
        let _is_not_annotative = reader.read_bit();
    }

    MTextData {
        insertion_point, normal, x_direction, rectangle_width, rectangle_height,
        height, attachment_point, drawing_direction, extents_height, extents_width,
        value, style_handle, linespacing_style, linespacing_factor, unknown_bit,
        background_flags,
    }
}

pub fn read_shape(reader: &mut DwgMergedReader) -> ShapeData {
    let insertion_point = reader.read_3bit_double();
    let size = reader.read_bit_double();
    let rotation = reader.read_bit_double();
    let relative_x_scale = reader.read_bit_double();
    let oblique_angle = reader.read_bit_double();
    let thickness = reader.read_bit_double();
    let shape_number = reader.read_bit_short();
    let normal = reader.read_3bit_double();
    let style_handle = reader.read_handle();
    ShapeData { insertion_point, size, rotation, relative_x_scale, oblique_angle, thickness, shape_number, normal, style_handle }
}

pub fn read_leader(reader: &mut DwgMergedReader, version: DwgVersion) -> LeaderData {
    let unknown_bit = reader.read_bit();
    let annotation_type = reader.read_bit_short();
    let path_type = reader.read_bit_short();

    let num_pts = safe_count(reader.read_bit_long());
    let mut vertices = Vec::with_capacity(num_pts as usize);
    for _ in 0..num_pts { vertices.push(reader.read_3bit_double()); }

    let origin = reader.read_3bit_double();
    let normal = reader.read_3bit_double();
    let horizontal_direction = reader.read_3bit_double();
    let block_offset = reader.read_3bit_double();
    let annotation_offset = reader.read_3bit_double();

    if version.r13_14_only() {
        let _dimgap = reader.read_bit_double();
    }

    let mut text_height = 0.0;
    let mut text_width = 0.0;
    if !version.r2010_plus() {
        text_height = reader.read_bit_double();
        text_width = reader.read_bit_double();
    }

    let hookline_on_x_dir = reader.read_bit();
    let arrowhead_on = reader.read_bit();

    if version.r13_14_only() {
        let _arrowhead_type = reader.read_bit_short();
        let _dimasz = reader.read_bit_double();
        let _unk1 = reader.read_bit();
        let _unk2 = reader.read_bit();
        let _unk3 = reader.read_bit_short();
        let _bbc = reader.read_bit_short();
        let _unk4 = reader.read_bit();
        let _unk5 = reader.read_bit();
    }

    if version.r2000_plus() {
        let _unk_bs = reader.read_bit_short();
        let _unk_b1 = reader.read_bit();
        let _unk_b2 = reader.read_bit();
    }

    let annotation_handle = reader.read_handle();
    let dimstyle_handle = reader.read_handle();

    LeaderData {
        unknown_bit, annotation_type, path_type, vertices,
        origin, normal, horizontal_direction, block_offset,
        annotation_offset, text_height, text_width,
        hookline_on_x_dir, arrowhead_on,
        annotation_handle, dimstyle_handle,
    }
}

pub fn read_tolerance(reader: &mut DwgMergedReader, version: DwgVersion) -> ToleranceData {
    if version.r13_14_only() {
        let _unk_short = reader.read_bit_short();
        let _text_height = reader.read_bit_double();
        let _dimgap = reader.read_bit_double();
    }

    let insertion_point = reader.read_3bit_double();
    let direction = reader.read_3bit_double();
    let normal = reader.read_3bit_double();
    let text = reader.read_variable_text();
    let dimstyle_handle = reader.read_handle();

    ToleranceData { insertion_point, direction, normal, text, dimstyle_handle }
}

// ════════════════════════════════════════════════════════════════════════
//  Result structs — Complex entities
// ════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct DimensionCommonData {
    pub version_byte: u8,
    pub normal: Vector3,
    pub text_middle_point: Vector3,
    pub flags_byte: u8,
    pub text: String,
    pub text_rotation: f64,
    pub horizontal_direction: f64,
    pub ins_scale: Vector3,
    pub ins_rotation: f64,
    pub attachment_point: i16,
    pub linespacing_style: i16,
    pub linespacing_factor: f64,
    pub actual_measurement: f64,
    pub unknown_bit: bool,
    pub flip_arrow1: bool,
    pub flip_arrow2: bool,
    pub insertion_point: Vector2,
    pub dimstyle_handle: u64,
    pub block_handle: u64,
}

#[derive(Debug, Clone)]
pub struct DimensionLinearData {
    pub common: DimensionCommonData,
    pub first_point: Vector3,
    pub second_point: Vector3,
    pub definition_point: Vector3,
    pub ext_line_rotation: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone)]
pub struct DimensionAlignedData {
    pub common: DimensionCommonData,
    pub first_point: Vector3,
    pub second_point: Vector3,
    pub definition_point: Vector3,
    pub ext_line_rotation: f64,
}

#[derive(Debug, Clone)]
pub struct DimensionRadiusData {
    pub common: DimensionCommonData,
    pub definition_point: Vector3,
    pub angle_vertex: Vector3,
    pub leader_length: f64,
}

#[derive(Debug, Clone)]
pub struct DimensionDiameterData {
    pub common: DimensionCommonData,
    pub definition_point: Vector3,
    pub angle_vertex: Vector3,
    pub leader_length: f64,
}

#[derive(Debug, Clone)]
pub struct DimensionAngular2LnData {
    pub common: DimensionCommonData,
    pub dimension_arc: Vector2,
    pub first_point: Vector3,
    pub second_point: Vector3,
    pub angle_vertex: Vector3,
    pub definition_point: Vector3,
}

#[derive(Debug, Clone)]
pub struct DimensionAngular3PtData {
    pub common: DimensionCommonData,
    pub definition_point: Vector3,
    pub first_point: Vector3,
    pub second_point: Vector3,
    pub angle_vertex: Vector3,
}

#[derive(Debug, Clone)]
pub struct DimensionOrdinateData {
    pub common: DimensionCommonData,
    pub definition_point: Vector3,
    pub feature_location: Vector3,
    pub leader_endpoint: Vector3,
    pub is_ordinate_type_x: bool,
}

#[derive(Debug, Clone)]
pub struct HatchBoundaryEdgeLine { pub start: Vector2, pub end: Vector2 }
#[derive(Debug, Clone)]
pub struct HatchBoundaryEdgeArc { pub center: Vector2, pub radius: f64, pub start_angle: f64, pub end_angle: f64, pub ccw: bool }
#[derive(Debug, Clone)]
pub struct HatchBoundaryEdgeEllipse { pub center: Vector2, pub major_endpoint: Vector2, pub minor_ratio: f64, pub start_angle: f64, pub end_angle: f64, pub ccw: bool }
#[derive(Debug, Clone)]
pub struct HatchBoundaryEdgeSpline { pub degree: i32, pub rational: bool, pub periodic: bool, pub knots: Vec<f64>, pub control_points: Vec<Vector3>, pub fit_points: Vec<Vector2>, pub start_tangent: Vector2, pub end_tangent: Vector2 }
#[derive(Debug, Clone)]
pub enum HatchEdge { Line(HatchBoundaryEdgeLine), Arc(HatchBoundaryEdgeArc), Ellipse(HatchBoundaryEdgeEllipse), Spline(HatchBoundaryEdgeSpline) }
#[derive(Debug, Clone)]
pub struct HatchBoundaryPath { pub flags: i32, pub edges: Vec<HatchEdge>, pub polyline_vertices: Vec<(Vector2, f64)>, pub polyline_closed: bool, pub boundary_handle_count: i32 }
#[derive(Debug, Clone)]
pub struct HatchPatternLine { pub angle: f64, pub base_point: Vector2, pub offset: Vector2, pub dashes: Vec<f64> }
#[derive(Debug, Clone)]
pub struct HatchData {
    pub gradient_enabled: bool,
    pub elevation: f64,
    pub normal: Vector3,
    pub pattern_name: String,
    pub is_solid: bool,
    pub is_associative: bool,
    pub paths: Vec<HatchBoundaryPath>,
    pub style: i16,
    pub pattern_type: i16,
    pub pattern_angle: f64,
    pub pattern_scale: f64,
    pub is_double: bool,
    pub pattern_lines: Vec<HatchPatternLine>,
    pub pixel_size: f64,
    pub seed_points: Vec<Vector2>,
}

#[derive(Debug, Clone)]
pub struct ViewportData {
    pub center: Vector3,
    pub width: f64,
    pub height: f64,
    pub view_target: Vector3,
    pub view_direction: Vector3,
    pub twist_angle: f64,
    pub view_height: f64,
    pub lens_length: f64,
    pub front_clip_z: f64,
    pub back_clip_z: f64,
    pub snap_angle: f64,
    pub view_center: Vector2,
    pub snap_base: Vector2,
    pub snap_spacing: Vector2,
    pub grid_spacing: Vector2,
    pub circle_sides: i16,
    pub frozen_layer_count: i32,
    pub status_flags: i32,
    pub render_mode: u8,
}

#[derive(Debug, Clone)]
pub struct Polyline2DData {
    pub flags: i16,
    pub start_width: f64,
    pub end_width: f64,
    pub thickness: f64,
    pub elevation: f64,
    pub normal: Vector3,
    pub owned_count: i32,
}

#[derive(Debug, Clone)]
pub struct Vertex2DData {
    pub flags: u8,
    pub x: f64, pub y: f64, pub z: f64,
    pub start_width: f64,
    pub end_width: f64,
    pub bulge: f64,
    pub vertex_id: i32,
    pub tangent_dir: f64,
}

#[derive(Debug, Clone)]
pub struct Polyline3DData {
    pub smooth_type: u8,
    pub closed_flag: u8,
    pub owned_count: i32,
}

#[derive(Debug, Clone)]
pub struct Vertex3DData {
    pub flags: u8,
    pub position: Vector3,
}

#[derive(Debug, Clone)]
pub struct MLineData {
    pub scale_factor: f64,
    pub justification: u8,
    pub start_point: Vector3,
    pub normal: Vector3,
    pub openclosed: i16,
    pub lines_in_style: u8,
    pub vertex_count: i16,
    pub style_handle: u64,
}

#[derive(Debug, Clone)]
pub struct MeshData {
    pub version: i16,
    pub blend_crease: bool,
    pub subdivision_level: i32,
    pub vertices: Vec<Vector3>,
    pub faces: Vec<Vec<i32>>,
    pub edges: Vec<(i32, i32)>,
    pub crease_values: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct RasterImageData {
    pub class_version: i32,
    pub insertion_point: Vector3,
    pub u_vector: Vector3,
    pub v_vector: Vector3,
    pub size: Vector2,
    pub flags: i16,
    pub clipping_enabled: bool,
    pub brightness: u8,
    pub contrast: u8,
    pub fade: u8,
    pub clip_inverted: bool,
    pub clip_type: i16,
    pub definition_handle: u64,
    pub reactor_handle: u64,
}

#[derive(Debug, Clone)]
pub struct Ole2FrameData {
    pub version: i16,
    pub mode: i16,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct AttributeCommonData {
    pub text_data: TextEntityData,
    pub att_version: u8,
    pub att_type: u8,
    pub tag: String,
    pub field_length: i16,
    pub flags: u8,
    pub lock_position: bool,
}

// ════════════════════════════════════════════════════════════════════════
//  Reader functions — Complex entities
// ════════════════════════════════════════════════════════════════════════

/// Read common dimension data shared by all dimension types.
pub fn read_common_dimension_data(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
    _dxf_version: DxfVersion,
) -> DimensionCommonData {
    let version_byte = if version.r2010_plus() { reader.read_byte() } else { 0 };
    let normal = reader.read_3bit_double();
    let text_mid = reader.read_2raw_double();
    let text_mid_z = reader.read_bit_double();
    let flags_byte = reader.read_byte();
    let text = reader.read_variable_text();
    let text_rotation = reader.read_bit_double();
    let horizontal_direction = reader.read_bit_double();
    let ins_scale = reader.read_3bit_double();
    let ins_rotation = reader.read_bit_double();

    let mut attachment_point = 0i16;
    let mut linespacing_style = 1i16;
    let mut linespacing_factor = 1.0;
    let mut actual_measurement = 0.0;
    if version.r2000_plus() {
        attachment_point = reader.read_bit_short();
        linespacing_style = reader.read_bit_short();
        linespacing_factor = reader.read_bit_double();
        actual_measurement = reader.read_bit_double();
    }

    let mut unknown_bit = false;
    let mut flip_arrow1 = false;
    let mut flip_arrow2 = false;
    if version.r2007_plus() {
        unknown_bit = reader.read_bit();
        flip_arrow1 = reader.read_bit();
        flip_arrow2 = reader.read_bit();
    }

    let insertion_point = reader.read_2raw_double();
    let dimstyle_handle = reader.read_handle();
    let block_handle = reader.read_handle();

    DimensionCommonData {
        version_byte, normal,
        text_middle_point: Vector3::new(text_mid.x, text_mid.y, text_mid_z),
        flags_byte, text, text_rotation, horizontal_direction,
        ins_scale, ins_rotation,
        attachment_point, linespacing_style, linespacing_factor,
        actual_measurement, unknown_bit, flip_arrow1, flip_arrow2,
        insertion_point, dimstyle_handle, block_handle,
    }
}

pub fn read_dimension_linear(reader: &mut DwgMergedReader, version: DwgVersion, dxf_version: DxfVersion) -> DimensionLinearData {
    let common = read_common_dimension_data(reader, version, dxf_version);
    let first_point = reader.read_3bit_double();
    let second_point = reader.read_3bit_double();
    let definition_point = reader.read_3bit_double();
    let ext_line_rotation = reader.read_bit_double();
    let rotation = reader.read_bit_double();
    DimensionLinearData { common, first_point, second_point, definition_point, ext_line_rotation, rotation }
}

pub fn read_dimension_aligned(reader: &mut DwgMergedReader, version: DwgVersion, dxf_version: DxfVersion) -> DimensionAlignedData {
    let common = read_common_dimension_data(reader, version, dxf_version);
    let first_point = reader.read_3bit_double();
    let second_point = reader.read_3bit_double();
    let definition_point = reader.read_3bit_double();
    let ext_line_rotation = reader.read_bit_double();
    DimensionAlignedData { common, first_point, second_point, definition_point, ext_line_rotation }
}

pub fn read_dimension_radius(reader: &mut DwgMergedReader, version: DwgVersion, dxf_version: DxfVersion) -> DimensionRadiusData {
    let common = read_common_dimension_data(reader, version, dxf_version);
    let definition_point = reader.read_3bit_double();
    let angle_vertex = reader.read_3bit_double();
    let leader_length = reader.read_bit_double();
    DimensionRadiusData { common, definition_point, angle_vertex, leader_length }
}

pub fn read_dimension_diameter(reader: &mut DwgMergedReader, version: DwgVersion, dxf_version: DxfVersion) -> DimensionDiameterData {
    let common = read_common_dimension_data(reader, version, dxf_version);
    let definition_point = reader.read_3bit_double();
    let angle_vertex = reader.read_3bit_double();
    let leader_length = reader.read_bit_double();
    DimensionDiameterData { common, definition_point, angle_vertex, leader_length }
}

pub fn read_dimension_angular_2ln(reader: &mut DwgMergedReader, version: DwgVersion, dxf_version: DxfVersion) -> DimensionAngular2LnData {
    let common = read_common_dimension_data(reader, version, dxf_version);
    let dimension_arc = reader.read_2raw_double();
    let first_point = reader.read_3bit_double();
    let second_point = reader.read_3bit_double();
    let angle_vertex = reader.read_3bit_double();
    let definition_point = reader.read_3bit_double();
    DimensionAngular2LnData { common, dimension_arc, first_point, second_point, angle_vertex, definition_point }
}

pub fn read_dimension_angular_3pt(reader: &mut DwgMergedReader, version: DwgVersion, dxf_version: DxfVersion) -> DimensionAngular3PtData {
    let common = read_common_dimension_data(reader, version, dxf_version);
    let definition_point = reader.read_3bit_double();
    let first_point = reader.read_3bit_double();
    let second_point = reader.read_3bit_double();
    let angle_vertex = reader.read_3bit_double();
    DimensionAngular3PtData { common, definition_point, first_point, second_point, angle_vertex }
}

pub fn read_dimension_ordinate(reader: &mut DwgMergedReader, version: DwgVersion, dxf_version: DxfVersion) -> DimensionOrdinateData {
    let common = read_common_dimension_data(reader, version, dxf_version);
    let definition_point = reader.read_3bit_double();
    let feature_location = reader.read_3bit_double();
    let leader_endpoint = reader.read_3bit_double();
    let is_ordinate_type_x = reader.read_byte() == 1;
    DimensionOrdinateData { common, definition_point, feature_location, leader_endpoint, is_ordinate_type_x }
}

/// Read a hatch boundary path (both polyline and non-polyline variants).
pub fn read_hatch_boundary_path(reader: &mut DwgMergedReader, version: DwgVersion) -> HatchBoundaryPath {
    let flags = reader.read_bit_long();
    let is_polyline = (flags & 2) != 0;

    let mut edges = Vec::new();
    let mut polyline_vertices = Vec::new();
    let mut polyline_closed = false;

    if !is_polyline {
        let num_edges = safe_count(reader.read_bit_long());
        for _ in 0..num_edges {
            let edge_type = reader.read_byte();
            match edge_type {
                1 => {
                    let start = reader.read_2raw_double();
                    let end = reader.read_2raw_double();
                    edges.push(HatchEdge::Line(HatchBoundaryEdgeLine { start, end }));
                }
                2 => {
                    let center = reader.read_2raw_double();
                    let radius = reader.read_bit_double();
                    let start_angle = reader.read_bit_double();
                    let end_angle = reader.read_bit_double();
                    let ccw = reader.read_bit();
                    edges.push(HatchEdge::Arc(HatchBoundaryEdgeArc { center, radius, start_angle, end_angle, ccw }));
                }
                3 => {
                    let center = reader.read_2raw_double();
                    let major_endpoint = reader.read_2raw_double();
                    let minor_ratio = reader.read_bit_double();
                    let start_angle = reader.read_bit_double();
                    let end_angle = reader.read_bit_double();
                    let ccw = reader.read_bit();
                    edges.push(HatchEdge::Ellipse(HatchBoundaryEdgeEllipse { center, major_endpoint, minor_ratio, start_angle, end_angle, ccw }));
                }
                4 => {
                    let degree = reader.read_bit_long();
                    let rational = reader.read_bit();
                    let periodic = reader.read_bit();
                    let num_knots = safe_count(reader.read_bit_long());
                    let num_ctrl = safe_count(reader.read_bit_long());
                    let mut knots = Vec::new();
                    for _ in 0..num_knots { knots.push(reader.read_bit_double()); }
                    let mut control_points = Vec::new();
                    for _ in 0..num_ctrl {
                        let pt = reader.read_2raw_double();
                        let w = if rational { reader.read_bit_double() } else { 1.0 };
                        control_points.push(Vector3::new(pt.x, pt.y, w));
                    }
                    let mut fit_points = Vec::new();
                    let mut start_tangent = Vector2::ZERO;
                    let mut end_tangent = Vector2::ZERO;
                    if version.r2010_plus() {
                        let num_fit = safe_count(reader.read_bit_long());
                        if num_fit > 0 {
                            for _ in 0..num_fit { fit_points.push(reader.read_2raw_double()); }
                            start_tangent = reader.read_2raw_double();
                            end_tangent = reader.read_2raw_double();
                        }
                    }
                    edges.push(HatchEdge::Spline(HatchBoundaryEdgeSpline { degree, rational, periodic, knots, control_points, fit_points, start_tangent, end_tangent }));
                }
                _ => {}
            }
        }
    } else {
        let has_bulge = reader.read_bit();
        polyline_closed = reader.read_bit();
        let num_verts = safe_count(reader.read_bit_long());
        for _ in 0..num_verts {
            let pt = reader.read_2raw_double();
            let bulge = if has_bulge { reader.read_bit_double() } else { 0.0 };
            polyline_vertices.push((pt, bulge));
        }
    }

    let boundary_handle_count = reader.read_bit_long();

    HatchBoundaryPath { flags, edges, polyline_vertices, polyline_closed, boundary_handle_count }
}

pub fn read_hatch(reader: &mut DwgMergedReader, version: DwgVersion) -> HatchData {
    let mut gradient_enabled = false;
    if version.r2004_plus() {
        let is_gradient = reader.read_bit_long();
        gradient_enabled = is_gradient != 0;
        // Skip gradient fields (reserve + angle + shift + single_color + tint)
        let _reserved = reader.read_bit_long();
        let _angle = reader.read_bit_double();
        let _shift = reader.read_bit_double();
        let _single_color = reader.read_bit_long();
        let _tint = reader.read_bit_double();
        let num_colors = safe_count(reader.read_bit_long());
        for _ in 0..num_colors {
            let _value = reader.read_bit_double();
            let _color = reader.read_cm_color();
        }
        let _grad_name = reader.read_variable_text();
    }

    let elevation = reader.read_bit_double();
    let normal = reader.read_3bit_double();
    let pattern_name = reader.read_variable_text();
    let is_solid = reader.read_bit();
    let is_associative = reader.read_bit();

    let num_paths = safe_count(reader.read_bit_long());
    let mut paths = Vec::new();
    let mut has_derived = false;
    for _ in 0..num_paths {
        let p = read_hatch_boundary_path(reader, version);
        if (p.flags & 4) != 0 { has_derived = true; }
        paths.push(p);
    }

    let style = reader.read_bit_short();
    let pattern_type = reader.read_bit_short();

    let mut pattern_angle = 0.0;
    let mut pattern_scale = 1.0;
    let mut is_double = false;
    let mut pattern_lines = Vec::new();
    if !is_solid {
        pattern_angle = reader.read_bit_double();
        pattern_scale = reader.read_bit_double();
        is_double = reader.read_bit();
        let num_lines = reader.read_bit_short();
        for _ in 0..num_lines {
            let angle = reader.read_bit_double();
            let base_pt = reader.read_2bit_double();
            let offset = reader.read_2bit_double();
            let num_dashes = reader.read_bit_short();
            let mut dashes = Vec::new();
            for _ in 0..num_dashes { dashes.push(reader.read_bit_double()); }
            pattern_lines.push(HatchPatternLine { angle, base_point: base_pt, offset, dashes });
        }
    }

    let pixel_size = if has_derived { reader.read_bit_double() } else { 0.0 };

    let num_seeds = safe_count(reader.read_bit_long());
    let mut seed_points = Vec::new();
    for _ in 0..num_seeds { seed_points.push(reader.read_2raw_double()); }

    // boundary handles are read externally (for each path, path.boundary_handle_count handles)

    HatchData {
        gradient_enabled, elevation, normal, pattern_name, is_solid, is_associative,
        paths, style, pattern_type, pattern_angle, pattern_scale, is_double,
        pattern_lines, pixel_size, seed_points,
    }
}

pub fn read_viewport(reader: &mut DwgMergedReader, version: DwgVersion, _dxf_version: DxfVersion) -> ViewportData {
    let center = reader.read_3bit_double();
    let width = reader.read_bit_double();
    let height = reader.read_bit_double();

    let mut view_target = Vector3::ZERO;
    let mut view_direction = Vector3::UNIT_Z;
    let mut twist_angle = 0.0;
    let mut view_height = 0.0;
    let mut lens_length = 50.0;
    let mut front_clip_z = 0.0;
    let mut back_clip_z = 0.0;
    let mut snap_angle = 0.0;
    let mut view_center = Vector2::ZERO;
    let mut snap_base = Vector2::ZERO;
    let mut snap_spacing = Vector2::ZERO;
    let mut grid_spacing = Vector2::ZERO;
    let mut circle_sides = 100i16;
    let mut frozen_layer_count = 0i32;
    let mut status_flags = 0i32;
    let mut render_mode = 0u8;

    if version.r2000_plus() {
        view_target = reader.read_3bit_double();
        view_direction = reader.read_3bit_double();
        twist_angle = reader.read_bit_double();
        view_height = reader.read_bit_double();
        lens_length = reader.read_bit_double();
        front_clip_z = reader.read_bit_double();
        back_clip_z = reader.read_bit_double();
        snap_angle = reader.read_bit_double();
        view_center = reader.read_2raw_double();
        snap_base = reader.read_2raw_double();
        snap_spacing = reader.read_2raw_double();
        grid_spacing = reader.read_2raw_double();
        circle_sides = reader.read_bit_short();
    }
    if version.r2007_plus() {
        let _grid_major = reader.read_bit_short();
    }
    if version.r2000_plus() {
        frozen_layer_count = reader.read_bit_long();
        status_flags = reader.read_bit_long();
        let _style_sheet = reader.read_variable_text();
        render_mode = reader.read_byte();
        let _ucs_at_origin = reader.read_bit();
        let _ucs_per_viewport = reader.read_bit();
        let _ucs_origin = reader.read_3bit_double();
        let _ucs_x_axis = reader.read_3bit_double();
        let _ucs_y_axis = reader.read_3bit_double();
        let _ucs_elevation = reader.read_bit_double();
        let _ucs_ortho_type = reader.read_bit_short();
    }
    if version.r2004_plus() {
        let _shade_plot_mode = reader.read_bit_short();
    }
    if version.r2007_plus() {
        let _default_lighting = reader.read_bit();
        let _default_lighting_type = reader.read_byte();
        let _brightness = reader.read_bit_double();
        let _contrast = reader.read_bit_double();
        let _ambient_color = reader.read_cm_color();
    }

    ViewportData {
        center, width, height, view_target, view_direction,
        twist_angle, view_height, lens_length, front_clip_z, back_clip_z,
        snap_angle, view_center, snap_base, snap_spacing, grid_spacing,
        circle_sides, frozen_layer_count, status_flags, render_mode,
    }
}

pub fn read_polyline2d(reader: &mut DwgMergedReader, version: DwgVersion) -> Polyline2DData {
    let flags = reader.read_bit_short();
    let start_width = reader.read_bit_double();
    let end_width = reader.read_bit_double();
    let thickness = reader.read_bit_thickness();
    let elevation = reader.read_bit_double();
    let normal = reader.read_bit_extrusion();
    let owned_count = if version.r2004_plus() { reader.read_bit_long() } else { 0 };
    Polyline2DData { flags, start_width, end_width, thickness, elevation, normal, owned_count }
}

pub fn read_vertex2d(reader: &mut DwgMergedReader, version: DwgVersion) -> Vertex2DData {
    let flags = reader.read_byte();
    let x = reader.read_bit_double();
    let y = reader.read_bit_double();
    let z = reader.read_bit_double();
    let sw = reader.read_bit_double();
    let (start_width, end_width) = if sw < 0.0 {
        (-sw, -sw) // negative = both widths equal
    } else {
        let ew = reader.read_bit_double();
        (sw, ew)
    };
    let bulge = reader.read_bit_double();
    let vertex_id = if version.r2010_plus() { reader.read_bit_long() } else { 0 };
    let tangent_dir = reader.read_bit_double();
    Vertex2DData { flags, x, y, z, start_width, end_width, bulge, vertex_id, tangent_dir }
}

pub fn read_polyline3d(reader: &mut DwgMergedReader, version: DwgVersion) -> Polyline3DData {
    let smooth_type = reader.read_byte();
    let closed_flag = reader.read_byte();
    let owned_count = if version.r2004_plus() { reader.read_bit_long() } else { 0 };
    Polyline3DData { smooth_type, closed_flag, owned_count }
}

pub fn read_vertex3d(reader: &mut DwgMergedReader) -> Vertex3DData {
    let flags = reader.read_byte();
    let position = reader.read_3bit_double();
    Vertex3DData { flags, position }
}

pub fn read_polyface_mesh(reader: &mut DwgMergedReader, version: DwgVersion) -> (i16, i16, i32) {
    let num_verts = reader.read_bit_short();
    let num_faces = reader.read_bit_short();
    let owned_count = if version.r2004_plus() { reader.read_bit_long() } else { 0 };
    (num_verts, num_faces, owned_count)
}

pub fn read_polygon_mesh(reader: &mut DwgMergedReader, version: DwgVersion) -> (i16, i16, i16, i16, i16, i16, i32) {
    let flags = reader.read_bit_short();
    let smooth_type = reader.read_bit_short();
    let m_count = reader.read_bit_short();
    let n_count = reader.read_bit_short();
    let m_smooth = reader.read_bit_short();
    let n_smooth = reader.read_bit_short();
    let owned_count = if version.r2004_plus() { reader.read_bit_long() } else { 0 };
    (flags, smooth_type, m_count, n_count, m_smooth, n_smooth, owned_count)
}

pub fn read_seqend(_reader: &mut DwgMergedReader) {
    // SEQEND has no entity-specific data
}

pub fn read_mline(reader: &mut DwgMergedReader) -> MLineData {
    let scale_factor = reader.read_bit_double();
    let justification = reader.read_byte();
    let start_point = reader.read_3bit_double();
    let normal = reader.read_3bit_double();
    let openclosed = reader.read_bit_short();
    let lines_in_style = reader.read_byte();
    let vertex_count = reader.read_bit_short();

    // Read vertices (position + direction + miter + segments)
    for _ in 0..vertex_count {
        let _pos = reader.read_3bit_double();
        let _dir = reader.read_3bit_double();
        let _miter = reader.read_3bit_double();
        for _ in 0..lines_in_style {
            let num_params = reader.read_bit_short();
            for _ in 0..num_params { let _p = reader.read_bit_double(); }
            let num_area = reader.read_bit_short();
            for _ in 0..num_area { let _p = reader.read_bit_double(); }
        }
    }

    let style_handle = reader.read_handle();

    MLineData { scale_factor, justification, start_point, normal, openclosed, lines_in_style, vertex_count, style_handle }
}

pub fn read_mesh(reader: &mut DwgMergedReader) -> MeshData {
    let version = reader.read_bit_short();
    let blend_crease = reader.read_bit();
    let subdivision_level = reader.read_bit_long();

    let num_verts = safe_count(reader.read_bit_long());
    let mut vertices = Vec::with_capacity(num_verts as usize);
    for _ in 0..num_verts { vertices.push(reader.read_3bit_double()); }

    let total_face_data = safe_count(reader.read_bit_long());
    let mut faces = Vec::new();
    let mut i = 0;
    while i < total_face_data {
        let n = safe_count(reader.read_bit_long());
        i += 1;
        let mut face = Vec::new();
        for _ in 0..n {
            face.push(reader.read_bit_long());
            i += 1;
        }
        faces.push(face);
    }

    let num_edges = safe_count(reader.read_bit_long());
    let mut edges = Vec::with_capacity(num_edges as usize);
    for _ in 0..num_edges {
        let s = reader.read_bit_long();
        let e = reader.read_bit_long();
        edges.push((s, e));
    }

    let num_creases = safe_count(reader.read_bit_long());
    let mut crease_values = Vec::with_capacity(num_creases as usize);
    for _ in 0..num_creases { crease_values.push(reader.read_bit_double()); }

    let _trailing = reader.read_bit_long();

    MeshData { version, blend_crease, subdivision_level, vertices, faces, edges, crease_values }
}

pub fn read_raster_image(reader: &mut DwgMergedReader, version: DwgVersion) -> RasterImageData {
    let class_version = reader.read_bit_long();
    let insertion_point = reader.read_3bit_double();
    let u_vector = reader.read_3bit_double();
    let v_vector = reader.read_3bit_double();
    let size = reader.read_2raw_double();
    let flags = reader.read_bit_short();
    let clipping_enabled = reader.read_bit();
    let brightness = reader.read_byte();
    let contrast = reader.read_byte();
    let fade = reader.read_byte();
    let clip_inverted = if version.r2010_plus() { reader.read_bit() } else { false };

    // Clip boundary
    let clip_type = reader.read_bit_short();
    if clip_type == 1 {
        // Rectangular: 2 fixed vertices
        let _pt1 = reader.read_2raw_double();
        let _pt2 = reader.read_2raw_double();
    } else {
        // Polygonal
        let n = safe_count(reader.read_bit_long());
        for _ in 0..n { let _pt = reader.read_2raw_double(); }
    }

    let definition_handle = reader.read_handle();
    let reactor_handle = reader.read_handle();

    RasterImageData {
        class_version, insertion_point, u_vector, v_vector, size,
        flags, clipping_enabled, brightness, contrast, fade, clip_inverted,
        clip_type, definition_handle, reactor_handle,
    }
}

pub fn read_wipeout(reader: &mut DwgMergedReader, version: DwgVersion) -> RasterImageData {
    // Wipeout uses the same data layout as RasterImage
    read_raster_image(reader, version)
}

pub fn read_ole2frame(reader: &mut DwgMergedReader, version: DwgVersion) -> Ole2FrameData {
    let ver = reader.read_bit_short();
    let mode = if version.r2000_plus() { reader.read_bit_short() } else { 0 };
    let data_len = safe_count(reader.read_bit_long());
    let mut data = Vec::with_capacity(data_len as usize);
    for _ in 0..data_len { data.push(reader.read_byte()); }
    if version.r2000_plus() {
        let _trailing = reader.read_byte();
    }
    Ole2FrameData { version: ver, mode, data }
}

pub fn read_attribute_definition(reader: &mut DwgMergedReader, version: DwgVersion, dxf_version: DxfVersion) -> AttributeCommonData {
    let mut text_data = read_text_entity_data(reader, version);
    text_data.style_handle = reader.read_handle();

    let att_version = if version.r2010_plus() { reader.read_byte() } else { 0 };
    let att_type = if version.r2018_plus(dxf_version) { reader.read_byte() } else { 1 };

    let tag = reader.read_variable_text();
    let field_length = reader.read_bit_short();
    let flags = reader.read_byte();
    let lock_position = if version.r2007_plus() { reader.read_bit() } else { false };

    // AttDef-specific: second version byte + prompt
    if version.r2010_plus() {
        let _version2 = reader.read_byte();
    }
    let _prompt = reader.read_variable_text();

    AttributeCommonData { text_data, att_version, att_type, tag, field_length, flags, lock_position }
}

pub fn read_attribute_entity(reader: &mut DwgMergedReader, version: DwgVersion, dxf_version: DxfVersion) -> AttributeCommonData {
    let mut text_data = read_text_entity_data(reader, version);
    text_data.style_handle = reader.read_handle();

    let att_version = if version.r2010_plus() { reader.read_byte() } else { 0 };
    let att_type = if version.r2018_plus(dxf_version) { reader.read_byte() } else { 1 };

    let tag = reader.read_variable_text();
    let field_length = reader.read_bit_short();
    let flags = reader.read_byte();
    let lock_position = if version.r2007_plus() { reader.read_bit() } else { false };

    AttributeCommonData { text_data, att_version, att_type, tag, field_length, flags, lock_position }
}

// ════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::dwg::dwg_stream_writers::merged_writer::DwgMergedWriter;
    use crate::io::dwg::dwg_version::DwgVersion;
    use crate::types::DxfVersion;

    fn make_reader(dwg: DwgVersion, dxf: DxfVersion, f: impl FnOnce(&mut DwgMergedWriter)) -> DwgMergedReader {
        let mut writer = DwgMergedWriter::new(dwg, dxf);
        f(&mut writer);
        let data = writer.merge();
        let hsb = writer.handle_start_bits();
        DwgMergedReader::new(data, dxf, hsb)
    }

    #[test]
    fn test_point_roundtrip() {
        let v = DwgVersion::AC15;
        let d = DxfVersion::AC1015;
        let mut r = make_reader(v, d, |w| {
            w.write_3bit_double(Vector3::new(1.0, 2.0, 3.0));
            w.write_bit_thickness(0.5);
            w.write_bit_extrusion(Vector3::UNIT_Z);
            w.write_bit_double(45.0);
        });
        let pt = read_point(&mut r);
        assert_eq!(pt.location, Vector3::new(1.0, 2.0, 3.0));
        assert_eq!(pt.thickness, 0.5);
        assert_eq!(pt.x_axis_angle, 45.0);
    }

    #[test]
    fn test_line_roundtrip_r2000() {
        let v = DwgVersion::AC15;
        let d = DxfVersion::AC1015;
        let mut r = make_reader(v, d, |w| {
            w.write_bit(false); // z_are_zero = false
            w.write_raw_double(1.0); // start.x
            w.write_bit_double_with_default(4.0, 1.0); // end.x
            w.write_raw_double(2.0); // start.y
            w.write_bit_double_with_default(5.0, 2.0); // end.y
            w.write_raw_double(3.0); // start.z
            w.write_bit_double_with_default(6.0, 3.0); // end.z
            w.write_bit_thickness(0.0);
            w.write_bit_extrusion(Vector3::UNIT_Z);
        });
        let ln = read_line(&mut r, v);
        assert_eq!(ln.start, Vector3::new(1.0, 2.0, 3.0));
        assert_eq!(ln.end, Vector3::new(4.0, 5.0, 6.0));
    }

    #[test]
    fn test_circle_roundtrip() {
        let v = DwgVersion::AC15;
        let d = DxfVersion::AC1015;
        let mut r = make_reader(v, d, |w| {
            w.write_3bit_double(Vector3::new(10.0, 20.0, 0.0));
            w.write_bit_double(5.0);
            w.write_bit_thickness(0.0);
            w.write_bit_extrusion(Vector3::UNIT_Z);
        });
        let c = read_circle(&mut r);
        assert_eq!(c.center, Vector3::new(10.0, 20.0, 0.0));
        assert_eq!(c.radius, 5.0);
    }

    #[test]
    fn test_ellipse_roundtrip() {
        let v = DwgVersion::AC15;
        let d = DxfVersion::AC1015;
        let mut r = make_reader(v, d, |w| {
            w.write_3bit_double(Vector3::new(5.0, 5.0, 0.0));
            w.write_3bit_double(Vector3::new(10.0, 0.0, 0.0));
            w.write_3bit_double(Vector3::UNIT_Z);
            w.write_bit_double(0.5);
            w.write_bit_double(0.0);
            w.write_bit_double(std::f64::consts::TAU);
        });
        let e = read_ellipse(&mut r);
        assert_eq!(e.center, Vector3::new(5.0, 5.0, 0.0));
        assert_eq!(e.major_axis, Vector3::new(10.0, 0.0, 0.0));
        assert_eq!(e.minor_axis_ratio, 0.5);
    }

    #[test]
    fn test_insert_roundtrip_r2000() {
        let v = DwgVersion::AC15;
        let d = DxfVersion::AC1015;
        let mut r = make_reader(v, d, |w| {
            w.write_3bit_double(Vector3::new(100.0, 200.0, 0.0));
            w.write_2bits(3); // all-ones scale
            w.write_bit_double(0.0); // rotation
            w.write_3bit_double(Vector3::UNIT_Z); // normal
            w.write_bit(false); // has_attribs
            w.write_handle(crate::io::dwg::dwg_reference_type::DwgReferenceType::HardPointer, 0x50);
        });
        let ins = read_insert(&mut r, v);
        assert_eq!(ins.insert_point, Vector3::new(100.0, 200.0, 0.0));
        assert_eq!(ins.x_scale, 1.0);
        assert_eq!(ins.y_scale, 1.0);
        assert_eq!(ins.z_scale, 1.0);
        assert_eq!(ins.block_handle, 0x50);
    }

    #[test]
    fn test_spline_roundtrip_scenario1() {
        let v = DwgVersion::AC15;
        let d = DxfVersion::AC1015;
        let mut r = make_reader(v, d, |w| {
            w.write_bit_long(1); // scenario
            w.write_bit_long(3); // degree
            w.write_bit(false); // rational
            w.write_bit(false); // closed
            w.write_bit(false); // periodic
            w.write_bit_double(1e-10); // knot_tol
            w.write_bit_double(1e-10); // ctrl_tol
            w.write_bit_long(6); // num_knots
            w.write_bit_long(3); // num_ctrl
            w.write_bit(false); // has_weights
            for k in &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0] {
                w.write_bit_double(*k);
            }
            w.write_3bit_double(Vector3::new(0.0, 0.0, 0.0));
            w.write_3bit_double(Vector3::new(5.0, 5.0, 0.0));
            w.write_3bit_double(Vector3::new(10.0, 0.0, 0.0));
        });
        let sp = read_spline(&mut r, v, d);
        assert_eq!(sp.scenario, 1);
        assert_eq!(sp.degree, 3);
        assert_eq!(sp.knots.len(), 6);
        assert_eq!(sp.control_points.len(), 3);
    }
}
