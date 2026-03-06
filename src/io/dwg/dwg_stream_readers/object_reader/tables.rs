//! Table entry readers for DWG object section.
//!
//! Each reader is the exact inverse of the corresponding writer in
//! `dwg_stream_writers/object_writer/mod.rs`. They read the non-entity
//! common data (already consumed by the caller), then the table-entry-
//! specific fields, and return a struct with the parsed data.
//!
//! Based on ACadSharp's `DwgObjectReader.cs` table entry methods.

use crate::io::dwg::dwg_version::DwgVersion;
use crate::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use crate::types::{DxfVersion, Color, Vector2, Vector3};
use super::safe_count;

// ════════════════════════════════════════════════════════════════════════
//  Result structs for each table entry type
// ════════════════════════════════════════════════════════════════════════

/// Parsed table control object data.
#[derive(Debug, Clone)]
pub struct TableControlData {
    /// Entry count
    pub entry_count: i32,
    /// Entry handles (soft ownership)
    pub entry_handles: Vec<u64>,
}

/// Parsed BLOCK_CONTROL data (special — has *Model_Space and *Paper_Space).
#[derive(Debug, Clone)]
pub struct BlockControlData {
    /// Regular entry count (excludes *Model_Space, *Paper_Space)
    pub entry_count: i32,
    /// Regular entry handles
    pub entry_handles: Vec<u64>,
    /// *Model_Space handle
    pub model_space_handle: u64,
    /// *Paper_Space handle
    pub paper_space_handle: u64,
}

/// Parsed LAYER data.
#[derive(Debug, Clone)]
pub struct LayerData {
    pub name: String,
    pub frozen: bool,
    pub off: bool,
    pub frozen_in_new_vp: bool,
    pub locked: bool,
    pub plottable: bool,
    pub line_weight: i16,
    pub color: Color,
    pub xref_handle: u64,
    pub plotstyle_handle: Option<u64>,
    pub material_handle: Option<u64>,
    pub linetype_handle: u64,
    pub unknown_handle: Option<u64>,
}

/// Parsed text STYLE data.
#[derive(Debug, Clone)]
pub struct TextStyleData {
    pub name: String,
    pub is_shape_file: bool,
    pub is_vertical: bool,
    pub height: f64,
    pub width_factor: f64,
    pub oblique_angle: f64,
    pub generation: u8,
    pub last_height: f64,
    pub font_file: String,
    pub big_font_file: String,
    pub xref_handle: u64,
}

/// Parsed LTYPE data.
#[derive(Debug, Clone)]
pub struct LinetypeData {
    pub name: String,
    pub description: String,
    pub pattern_length: f64,
    pub alignment: u8,
    pub segments: Vec<LinetypeSegment>,
    pub xref_handle: u64,
    pub shape_handles: Vec<u64>,
}

/// A single dash/dot/space segment in a linetype.
#[derive(Debug, Clone)]
pub struct LinetypeSegment {
    pub length: f64,
    pub shape_number: i16,
    pub offset_x: f64,
    pub offset_y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub shape_flags: i16,
}

/// Parsed VIEW data.
#[derive(Debug, Clone)]
pub struct ViewData {
    pub name: String,
    pub height: f64,
    pub width: f64,
    pub center: Vector2,
    pub target: Vector3,
    pub direction: Vector3,
    pub twist_angle: f64,
    pub lens_length: f64,
    pub front_clip: f64,
    pub back_clip: f64,
    pub perspective: bool,
    pub front_clipping: bool,
    pub back_clipping: bool,
    pub front_clip_z: bool,
    pub render_mode: Option<u8>,
    pub paper_space: bool,
    pub view_control_handle: u64,
}

/// Parsed UCS data.
#[derive(Debug, Clone)]
pub struct UcsData {
    pub name: String,
    pub origin: Vector3,
    pub x_axis: Vector3,
    pub y_axis: Vector3,
    pub elevation: Option<f64>,
    pub ortho_view_type: Option<i16>,
    pub ortho_type: Option<i16>,
    pub ucs_control_handle: u64,
}

/// Parsed VPORT data.
#[derive(Debug, Clone)]
pub struct VPortData {
    pub name: String,
    pub view_height: f64,
    pub aspect_ratio_times_height: f64,
    pub view_center: Vector2,
    pub view_target: Vector3,
    pub view_direction: Vector3,
    pub view_twist: f64,
    pub lens_length: f64,
    pub front_clip: f64,
    pub back_clip: f64,
    pub lower_left: Vector2,
    pub upper_right: Vector2,
    pub ucsfollow: bool,
    pub circle_zoom: i16,
    pub fast_zoom: bool,
    pub ucsicon_lower: bool,
    pub ucsicon_origin: bool,
    pub grid_on: bool,
    pub grid_spacing: Vector2,
    pub snap_on: bool,
    pub snap_style: bool,
    pub snap_isopair: i16,
    pub snap_rotation: f64,
    pub snap_base: Vector2,
    pub snap_spacing: Vector2,
    pub xref_handle: u64,
}

/// Parsed APPID data.
#[derive(Debug, Clone)]
pub struct AppIdData {
    pub name: String,
    pub unknown_byte: u8,
    pub xref_handle: u64,
}

/// Parsed DIMSTYLE data.
#[derive(Debug, Clone)]
pub struct DimStyleData {
    pub name: String,
    // We store all the dimstyle fields
    pub dimpost: String,
    pub dimapost: String,
    pub dimscale: f64,
    pub dimasz: f64,
    pub dimexo: f64,
    pub dimdli: f64,
    pub dimexe: f64,
    pub dimrnd: f64,
    pub dimdle: f64,
    pub dimtp: f64,
    pub dimtm: f64,
    pub dimtol: bool,
    pub dimlim: bool,
    pub dimtih: bool,
    pub dimtoh: bool,
    pub dimse1: bool,
    pub dimse2: bool,
    pub dimtad: i16,
    pub dimzin: i16,
    pub dimazin: i16,
    pub dimtxt: f64,
    pub dimcen: f64,
    pub dimtsz: f64,
    pub dimaltf: f64,
    pub dimlfac: f64,
    pub dimtvp: f64,
    pub dimtfac: f64,
    pub dimgap: f64,
    pub dimaltrnd: f64,
    pub dimalt: bool,
    pub dimaltd: i16,
    pub dimtofl: bool,
    pub dimsah: bool,
    pub dimtix: bool,
    pub dimsoxd: bool,
    pub dimclrd: Color,
    pub dimclre: Color,
    pub dimclrt: Color,
    pub dimadec: i16,
    pub dimdec: i16,
    pub dimtdec: i16,
    pub dimaltu: i16,
    pub dimalttd: i16,
    pub dimaunit: i16,
    pub dimfrac: i16,
    pub dimlunit: i16,
    pub dimdsep: i16,
    pub dimtmove: i16,
    pub dimjust: i16,
    pub dimsd1: bool,
    pub dimsd2: bool,
    pub dimtolj: i16,
    pub dimtzin: i16,
    pub dimaltz: i16,
    pub dimalttz: i16,
    pub dimupt: bool,
    pub dimfit: i16,
    pub dimlwd: i16,
    pub dimlwe: i16,
    pub xref_handle: u64,
    pub dimtxsty_handle: u64,
    pub dimldrblk_handle: Option<u64>,
    pub dimblk_handle: Option<u64>,
    pub dimblk1_handle: Option<u64>,
    pub dimblk2_handle: Option<u64>,
}

/// Parsed BLOCK_HEADER (block record) data.
#[derive(Debug, Clone)]
pub struct BlockHeaderData {
    pub name: String,
    pub anonymous: bool,
    pub has_attributes: bool,
    pub is_xref: bool,
    pub is_xref_overlay: bool,
    pub is_loaded: Option<bool>,
    pub owned_object_count: Option<i32>,
    pub base_point: Vector3,
    pub xref_path: String,
    pub description: Option<String>,
    pub preview_data_size: Option<i32>,
    pub units: Option<i16>,
    pub explodable: Option<bool>,
    pub scale_uniformly: Option<u8>,
    pub null_handle: u64,
    pub block_entity_handle: u64,
    pub entity_handles: Vec<u64>,
    pub endblk_handle: u64,
    pub layout_handle: Option<u64>,
}

// ════════════════════════════════════════════════════════════════════════
//  Reader methods
// ════════════════════════════════════════════════════════════════════════

/// Read xref-dependant bits for a table entry.
/// - R13/R14: B (64-flag) + B (xdep)
/// - R2000-R2006: B (64-flag) + BS (xrefindex+1) + B (xdep)
/// - R2007+: BS (combined)
fn read_xref_dependant_bits(reader: &mut DwgMergedReader, version: DwgVersion) {
    if version.r2007_plus() {
        let _combined = reader.read_bit_short();
    } else if version.r2000_plus() {
        let _xref_64 = reader.read_bit();
        let _xref_index = reader.read_bit_short();
        let _xref_dep = reader.read_bit();
    } else {
        // R13/R14: only 64-flag and xdep
        let _xref_64 = reader.read_bit();
        let _xref_dep = reader.read_bit();
    }
}

/// Read a generic table control object (after non-entity common data).
pub fn read_table_control(
    reader: &mut DwgMergedReader,
) -> TableControlData {
    let entry_count = safe_count(reader.read_bit_long());
    let mut entry_handles = Vec::new();
    for _ in 0..entry_count {
        entry_handles.push(reader.read_handle());
    }
    TableControlData {
        entry_count,
        entry_handles,
    }
}

/// Read BLOCK_CONTROL data (special: has *Model_Space and *Paper_Space).
pub fn read_block_control(
    reader: &mut DwgMergedReader,
) -> BlockControlData {
    let entry_count = safe_count(reader.read_bit_long());
    let mut entry_handles = Vec::new();
    for _ in 0..entry_count {
        entry_handles.push(reader.read_handle());
    }
    let model_space_handle = reader.read_handle();
    let paper_space_handle = reader.read_handle();

    BlockControlData {
        entry_count,
        entry_handles,
        model_space_handle,
        paper_space_handle,
    }
}

/// Read LAYER table entry data (after non-entity common data).
pub fn read_layer(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
    dxf_version: DxfVersion,
) -> LayerData {
    let name = reader.read_variable_text();

    // Xref-dependant bits (version-dependent encoding)
    read_xref_dependant_bits(reader, version);

    let frozen;
    let off;
    let frozen_in_new_vp;
    let locked;
    let mut plottable = false;
    let mut line_weight: i16 = 0;

    if version.r2000_plus() {
        let values = reader.read_bit_short();
        let lw_index = ((values >> 5) & 0x1F) as u8;
        line_weight = crate::types::LineWeight::from_dwg_index(lw_index).as_i16();
        frozen = (values & 0b0001) != 0;
        off = (values & 0b0010) != 0;
        frozen_in_new_vp = (values & 0b0100) != 0;
        locked = (values & 0b1000) != 0;
        plottable = (values & 0b10000) != 0;
    } else {
        frozen = reader.read_bit();
        let on = reader.read_bit();
        off = !on;
        frozen_in_new_vp = reader.read_bit();
        locked = reader.read_bit();
    }

    let color = reader.read_cm_color();

    // External reference block handle
    let xref_handle = reader.read_handle();

    // R2000+: plotstyle handle
    let plotstyle_handle = if version.r2000_plus() {
        Some(reader.read_handle())
    } else {
        None
    };

    // R2007+: material handle
    let material_handle = if version.r2007_plus() {
        Some(reader.read_handle())
    } else {
        None
    };

    // Linetype handle
    let linetype_handle = reader.read_handle();

    // R2013+: unknown handle
    let unknown_handle = if version.r2013_plus(dxf_version) {
        Some(reader.read_handle())
    } else {
        None
    };

    LayerData {
        name, frozen, off, frozen_in_new_vp, locked, plottable,
        line_weight, color, xref_handle, plotstyle_handle,
        material_handle, linetype_handle, unknown_handle,
    }
}

/// Read text STYLE table entry data.
pub fn read_text_style(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
) -> TextStyleData {
    let name = reader.read_variable_text();
    read_xref_dependant_bits(reader, version);

    let is_shape_file = reader.read_bit();
    let is_vertical = reader.read_bit();
    let height = reader.read_bit_double();
    let width_factor = reader.read_bit_double();
    let oblique_angle = reader.read_bit_double();
    let generation = reader.read_byte();
    let last_height = reader.read_bit_double();
    let font_file = reader.read_variable_text();
    let big_font_file = reader.read_variable_text();

    let xref_handle = reader.read_handle();

    TextStyleData {
        name, is_shape_file, is_vertical, height, width_factor,
        oblique_angle, generation, last_height, font_file,
        big_font_file, xref_handle,
    }
}

/// Read LTYPE table entry data.
pub fn read_linetype(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
) -> LinetypeData {
    let name = reader.read_variable_text();
    read_xref_dependant_bits(reader, version);

    let description = reader.read_variable_text();
    let pattern_length = reader.read_bit_double();
    let alignment = reader.read_byte();
    let num_dashes = reader.read_byte() as usize;

    let mut segments = Vec::with_capacity(num_dashes);
    for _ in 0..num_dashes {
        let length = reader.read_bit_double();
        let shape_number = reader.read_bit_short();
        let offset_x = reader.read_raw_double();
        let offset_y = reader.read_raw_double();
        let scale = reader.read_bit_double();
        let rotation = reader.read_bit_double();
        let shape_flags = reader.read_bit_short();
        segments.push(LinetypeSegment {
            length, shape_number, offset_x, offset_y,
            scale, rotation, shape_flags,
        });
    }

    // R2004 and earlier: 256-byte text area
    if !version.r2007_plus() {
        for _ in 0..256 {
            reader.read_byte();
        }
    }

    // Xref handle
    let xref_handle = reader.read_handle();

    // Shape file handles per segment
    let mut shape_handles = Vec::with_capacity(num_dashes);
    for _ in 0..num_dashes {
        shape_handles.push(reader.read_handle());
    }

    LinetypeData {
        name, description, pattern_length, alignment,
        segments, xref_handle, shape_handles,
    }
}

/// Read VIEW table entry data.
pub fn read_view(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
) -> ViewData {
    let name = reader.read_variable_text();
    read_xref_dependant_bits(reader, version);

    let height = reader.read_bit_double();
    let width = reader.read_bit_double();
    let center = reader.read_2raw_double();
    let target = reader.read_3bit_double();
    let direction = reader.read_3bit_double();
    let twist_angle = reader.read_bit_double();
    let lens_length = reader.read_bit_double();
    let front_clip = reader.read_bit_double();
    let back_clip = reader.read_bit_double();

    // View mode (4 bits)
    let perspective = reader.read_bit();
    let front_clipping = reader.read_bit();
    let back_clipping = reader.read_bit();
    let front_clip_z = reader.read_bit();

    let render_mode = if version.r2000_plus() {
        Some(reader.read_byte())
    } else {
        None
    };

    if version.r2007_plus() {
        let _use_default_lights = reader.read_bit();
        let _default_lighting = reader.read_byte();
        let _brightness = reader.read_bit_double();
        let _contrast = reader.read_bit_double();
        let _ambient_color = reader.read_cm_color();
    }

    let paper_space = reader.read_bit();

    if version.r2000_plus() {
        let _is_ucs_associated = reader.read_bit();
    }

    let view_control_handle = reader.read_handle();

    if version.r2007_plus() {
        let _camera_plottable = reader.read_bit();
        let _bg_handle = reader.read_handle();
        let _live_section_handle = reader.read_handle();
        let _style_handle = reader.read_handle();
    }

    if version.r2007_plus() {
        let _sun_handle = reader.read_handle();
    }

    ViewData {
        name, height, width, center, target, direction,
        twist_angle, lens_length, front_clip, back_clip,
        perspective, front_clipping, back_clipping, front_clip_z,
        render_mode, paper_space, view_control_handle,
    }
}

/// Read UCS table entry data.
pub fn read_ucs(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
) -> UcsData {
    let name = reader.read_variable_text();
    read_xref_dependant_bits(reader, version);

    let origin = reader.read_3bit_double();
    let x_axis = reader.read_3bit_double();
    let y_axis = reader.read_3bit_double();

    let (elevation, ortho_view_type, ortho_type) = if version.r2000_plus() {
        let e = reader.read_bit_double();
        let ovt = reader.read_bit_short();
        let ot = reader.read_bit_short();
        (Some(e), Some(ovt), Some(ot))
    } else {
        (None, None, None)
    };

    let ucs_control_handle = reader.read_handle();

    if version.r2000_plus() {
        let _named_ucs = reader.read_handle();
        let _base_ucs = reader.read_handle();
    }

    UcsData {
        name, origin, x_axis, y_axis, elevation,
        ortho_view_type, ortho_type, ucs_control_handle,
    }
}

/// Read VPORT table entry data.
pub fn read_vport(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
) -> VPortData {
    let name = reader.read_variable_text();
    read_xref_dependant_bits(reader, version);

    let view_height = reader.read_bit_double();
    let aspect_ratio_times_height = reader.read_bit_double();
    let view_center = reader.read_2raw_double();
    let view_target = reader.read_3bit_double();
    let view_direction = reader.read_3bit_double();
    let view_twist = reader.read_bit_double();
    let lens_length = reader.read_bit_double();
    let front_clip = reader.read_bit_double();
    let back_clip = reader.read_bit_double();

    // View mode (4 bits)
    let _perspective = reader.read_bit();
    let _front_clipping = reader.read_bit();
    let _back_clipping = reader.read_bit();
    let _front_clip_z = reader.read_bit();

    // R2000+: render mode
    if version.r2000_plus() {
        let _render_mode = reader.read_byte();
    }

    // R2007+: lighting
    if version.r2007_plus() {
        let _use_default_lights = reader.read_bit();
        let _default_lighting = reader.read_byte();
        let _brightness = reader.read_bit_double();
        let _contrast = reader.read_bit_double();
        let _ambient_color = reader.read_cm_color();
    }

    // Common viewport fields
    let lower_left = reader.read_2raw_double();
    let upper_right = reader.read_2raw_double();

    let ucsfollow = reader.read_bit();
    let circle_zoom = reader.read_bit_short();
    let fast_zoom = reader.read_bit();
    let ucsicon_lower = reader.read_bit();
    let ucsicon_origin = reader.read_bit();
    let grid_on = reader.read_bit();
    let grid_spacing = reader.read_2raw_double();
    let snap_on = reader.read_bit();
    let snap_style = reader.read_bit();
    let snap_isopair = reader.read_bit_short();
    let snap_rotation = reader.read_bit_double();
    let snap_base = reader.read_2raw_double();
    let snap_spacing = reader.read_2raw_double();

    // R2000+: UCS fields
    if version.r2000_plus() {
        let _unknown = reader.read_bit();
        let _ucs_per_viewport = reader.read_bit();
        let _ucs_origin = reader.read_3bit_double();
        let _ucs_x_axis = reader.read_3bit_double();
        let _ucs_y_axis = reader.read_3bit_double();
        let _ucs_elevation = reader.read_bit_double();
        let _ucs_ortho_type = reader.read_bit_short();
    }

    // R2007+: grid flags
    if version.r2007_plus() {
        let _grid_flags = reader.read_bit_short();
        let _grid_major = reader.read_bit_short();
    }

    // External reference block handle
    let xref_handle = reader.read_handle();

    // R2007+: extra handles
    if version.r2007_plus() {
        let _bg_handle = reader.read_handle();
        let _visual_style_handle = reader.read_handle();
        let _sun_handle = reader.read_handle();
    }

    // R2000+: UCS handles
    if version.r2000_plus() {
        let _named_ucs_handle = reader.read_handle();
        let _base_ucs_handle = reader.read_handle();
    }

    VPortData {
        name, view_height, aspect_ratio_times_height,
        view_center, view_target, view_direction,
        view_twist, lens_length, front_clip, back_clip,
        lower_left, upper_right, ucsfollow, circle_zoom,
        fast_zoom, ucsicon_lower, ucsicon_origin, grid_on,
        grid_spacing, snap_on, snap_style, snap_isopair,
        snap_rotation, snap_base, snap_spacing, xref_handle,
    }
}

/// Read APPID table entry data.
pub fn read_appid(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
) -> AppIdData {
    let name = reader.read_variable_text();
    read_xref_dependant_bits(reader, version);

    let unknown_byte = reader.read_byte();
    let xref_handle = reader.read_handle();

    AppIdData {
        name, unknown_byte, xref_handle,
    }
}

/// Read DIMSTYLE table entry data.
pub fn read_dimstyle(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
    _dxf_version: DxfVersion,
) -> DimStyleData {
    let name = reader.read_variable_text();
    read_xref_dependant_bits(reader, version);

    // Defaults
    let mut ds = DimStyleData {
        name,
        dimpost: String::new(), dimapost: String::new(),
        dimscale: 1.0, dimasz: 0.18, dimexo: 0.0625, dimdli: 0.38,
        dimexe: 0.18, dimrnd: 0.0, dimdle: 0.0, dimtp: 0.0, dimtm: 0.0,
        dimtol: false, dimlim: false, dimtih: true, dimtoh: true,
        dimse1: false, dimse2: false, dimtad: 0, dimzin: 0, dimazin: 0,
        dimtxt: 0.18, dimcen: 0.09, dimtsz: 0.0, dimaltf: 25.4,
        dimlfac: 1.0, dimtvp: 0.0, dimtfac: 1.0, dimgap: 0.09,
        dimaltrnd: 0.0, dimalt: false, dimaltd: 2, dimtofl: false,
        dimsah: false, dimtix: false, dimsoxd: false,
        dimclrd: Color::from_index(0), dimclre: Color::from_index(0),
        dimclrt: Color::from_index(0),
        dimadec: 0, dimdec: 4, dimtdec: 4,
        dimaltu: 2, dimalttd: 2, dimaunit: 0, dimfrac: 0,
        dimlunit: 2, dimdsep: 46, dimtmove: 0, dimjust: 0,
        dimsd1: false, dimsd2: false, dimtolj: 1, dimtzin: 0,
        dimaltz: 0, dimalttz: 0, dimupt: false, dimfit: 3,
        dimlwd: 0, dimlwe: 0,
        xref_handle: 0, dimtxsty_handle: 0,
        dimldrblk_handle: None, dimblk_handle: None,
        dimblk1_handle: None, dimblk2_handle: None,
    };

    // R2000+
    if version.r2000_plus() {
        ds.dimpost = reader.read_variable_text();
        ds.dimapost = reader.read_variable_text();
        ds.dimscale = reader.read_bit_double();
        ds.dimasz = reader.read_bit_double();
        ds.dimexo = reader.read_bit_double();
        ds.dimdli = reader.read_bit_double();
        ds.dimexe = reader.read_bit_double();
        ds.dimrnd = reader.read_bit_double();
        ds.dimdle = reader.read_bit_double();
        ds.dimtp = reader.read_bit_double();
        ds.dimtm = reader.read_bit_double();
    }

    // R2007+
    if version.r2007_plus() {
        let _dimfxl = reader.read_bit_double();
        let _dimjogang = reader.read_bit_double();
        let _dimtfill = reader.read_bit_short();
        let _dimtfillclr = reader.read_cm_color();
    }

    // R2000+
    if version.r2000_plus() {
        ds.dimtol = reader.read_bit();
        ds.dimlim = reader.read_bit();
        ds.dimtih = reader.read_bit();
        ds.dimtoh = reader.read_bit();
        ds.dimse1 = reader.read_bit();
        ds.dimse2 = reader.read_bit();
        ds.dimtad = reader.read_bit_short();
        ds.dimzin = reader.read_bit_short();
        ds.dimazin = reader.read_bit_short();
    }

    // R2007+
    if version.r2007_plus() {
        let _dimarcsym = reader.read_bit_short();
    }

    // R2000+
    if version.r2000_plus() {
        ds.dimtxt = reader.read_bit_double();
        ds.dimcen = reader.read_bit_double();
        ds.dimtsz = reader.read_bit_double();
        ds.dimaltf = reader.read_bit_double();
        ds.dimlfac = reader.read_bit_double();
        ds.dimtvp = reader.read_bit_double();
        ds.dimtfac = reader.read_bit_double();
        ds.dimgap = reader.read_bit_double();
        ds.dimaltrnd = reader.read_bit_double();
        ds.dimalt = reader.read_bit();
        ds.dimaltd = reader.read_bit_short();
        ds.dimtofl = reader.read_bit();
        ds.dimsah = reader.read_bit();
        ds.dimtix = reader.read_bit();
        ds.dimsoxd = reader.read_bit();
        ds.dimclrd = reader.read_cm_color();
        ds.dimclre = reader.read_cm_color();
        ds.dimclrt = reader.read_cm_color();
        ds.dimadec = reader.read_bit_short();
        ds.dimdec = reader.read_bit_short();
        ds.dimtdec = reader.read_bit_short();
        ds.dimaltu = reader.read_bit_short();
        ds.dimalttd = reader.read_bit_short();
        ds.dimaunit = reader.read_bit_short();
        ds.dimfrac = reader.read_bit_short();
        ds.dimlunit = reader.read_bit_short();
        ds.dimdsep = reader.read_bit_short();
        ds.dimtmove = reader.read_bit_short();
        ds.dimjust = reader.read_bit_short();
        ds.dimsd1 = reader.read_bit();
        ds.dimsd2 = reader.read_bit();
        ds.dimtolj = reader.read_bit_short();
        ds.dimtzin = reader.read_bit_short();
        ds.dimaltz = reader.read_bit_short();
        ds.dimalttz = reader.read_bit_short();
        ds.dimupt = reader.read_bit();
        ds.dimfit = reader.read_bit_short();
    }

    // R2007+
    if version.r2007_plus() {
        let _dimfxlon = reader.read_bit();
    }

    // R2010+
    if version.r2010_plus() {
        let _dimtxtdirection = reader.read_bit();
        let _dimaltmzf = reader.read_bit_double();
        let _dimaltmzs = reader.read_variable_text();
        let _dimmzf = reader.read_bit_double();
        let _dimmzs = reader.read_variable_text();
    }

    // R2000+
    if version.r2000_plus() {
        ds.dimlwd = reader.read_bit_short();
        ds.dimlwe = reader.read_bit_short();
    }

    // Common: Unknown flag
    let _unknown_flag = reader.read_bit();

    // Handle references
    ds.xref_handle = reader.read_handle();
    ds.dimtxsty_handle = reader.read_handle();

    // R2000+
    if version.r2000_plus() {
        ds.dimldrblk_handle = Some(reader.read_handle());
        ds.dimblk_handle = Some(reader.read_handle());
        ds.dimblk1_handle = Some(reader.read_handle());
        ds.dimblk2_handle = Some(reader.read_handle());
    }

    // R2007+
    if version.r2007_plus() {
        let _dimltype = reader.read_handle();
        let _dimltex1 = reader.read_handle();
        let _dimltex2 = reader.read_handle();
    }

    ds
}

/// Read BLOCK_HEADER (block record) table entry data.
pub fn read_block_header(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
) -> BlockHeaderData {
    let name = reader.read_variable_text();
    read_xref_dependant_bits(reader, version);

    let anonymous = reader.read_bit();
    let has_attributes = reader.read_bit();
    let is_xref = reader.read_bit();
    let is_xref_overlay = reader.read_bit();

    let is_loaded = if version.r2000_plus() {
        Some(reader.read_bit())
    } else {
        None
    };

    let owned_object_count = if version.r2004_plus() && !is_xref && !is_xref_overlay {
        Some(safe_count(reader.read_bit_long()))
    } else {
        None
    };

    let base_point = reader.read_3bit_double();
    let xref_path = reader.read_variable_text();

    let mut description = None;
    let mut preview_data_size = None;
    if version.r2000_plus() {
        // Insert count bytes — read until 0
        loop {
            let b = reader.read_byte();
            if b == 0 { break; }
        }

        description = Some(reader.read_variable_text());
        preview_data_size = Some(safe_count(reader.read_bit_long()));

        // Skip preview data if present
        let pds = preview_data_size.unwrap_or(0);
        if pds > 0 {
            for _ in 0..pds {
                reader.read_byte();
            }
        }
    }

    let (units, explodable, scale_uniformly) = if version.r2007_plus() {
        let u = reader.read_bit_short();
        let e = reader.read_bit();
        let s = reader.read_byte();
        (Some(u), Some(e), Some(s))
    } else {
        (None, None, None)
    };

    // Handles
    let null_handle = reader.read_handle();
    let block_entity_handle = reader.read_handle();

    // R13-R2000: first/last entity handles
    let mut entity_handles = Vec::new();
    if version.r13_15_only() && !is_xref && !is_xref_overlay {
        let _first = reader.read_handle();
        let _last = reader.read_handle();
    }

    // R2004+: entity handles
    if version.r2004_plus() {
        let count = owned_object_count.unwrap_or(0);
        for _ in 0..count {
            entity_handles.push(reader.read_handle());
        }
    }

    let endblk_handle = reader.read_handle();

    let layout_handle = if version.r2000_plus() {
        Some(reader.read_handle())
    } else {
        None
    };

    BlockHeaderData {
        name, anonymous, has_attributes, is_xref, is_xref_overlay,
        is_loaded, owned_object_count, base_point, xref_path,
        description, preview_data_size, units, explodable,
        scale_uniformly, null_handle, block_entity_handle,
        entity_handles, endblk_handle, layout_handle,
    }
}

/// Parsed VPORT_ENTITY_CONTROL data (R13-R14 viewport entity control, type 70).
#[derive(Debug, Clone)]
pub struct VPortEntityControlData {
    /// Entry count
    pub entry_count: i32,
    /// Entry handles (soft ownership)
    pub entry_handles: Vec<u64>,
}

/// Parsed VPORT_ENTITY_HEADER data (R13-R14 viewport entity header, type 71).
#[derive(Debug, Clone)]
pub struct VPortEntityHeaderData {
    pub name: String,
    /// Flag bit
    pub flag: bool,
    /// Entity handle (hard pointer)
    pub entity_handle: u64,
}

/// Read VPORT_ENTITY_CONTROL (type 70) — R13-R14 viewport entity control.
/// Same structure as a generic table control.
pub fn read_vport_entity_control(
    reader: &mut DwgMergedReader,
) -> VPortEntityControlData {
    let entry_count = safe_count(reader.read_bit_long());
    let mut entry_handles = Vec::new();
    for _ in 0..entry_count {
        entry_handles.push(reader.read_handle());
    }
    VPortEntityControlData {
        entry_count,
        entry_handles,
    }
}

/// Read VPORT_ENTITY_HEADER (type 71) — R13-R14 viewport entity header.
pub fn read_vport_entity_header(
    reader: &mut DwgMergedReader,
    version: DwgVersion,
) -> VPortEntityHeaderData {
    let name = reader.read_variable_text();
    read_xref_dependant_bits(reader, version);

    let flag = reader.read_bit();
    let entity_handle = reader.read_handle();

    VPortEntityHeaderData {
        name,
        flag,
        entity_handle,
    }
}

// ════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::dwg::dwg_version::DwgVersion;
    use crate::io::dwg::dwg_stream_writers::merged_writer::DwgMergedWriter;
    use crate::io::dwg::dwg_reference_type::DwgReferenceType;

    /// Helper: write merged data with a writer, produce a reader.
    /// Uses the writer's handle_start_bits to properly set up the reader's
    /// handle stream position.
    fn make_reader(dwg: DwgVersion, dxf: DxfVersion, f: impl FnOnce(&mut DwgMergedWriter)) -> DwgMergedReader {
        let mut writer = DwgMergedWriter::new(dwg, dxf);
        f(&mut writer);
        let data = writer.merge();
        // handle_start_bits is the bit position where handle data begins
        // DwgMergedReader::new expects this position, not the count
        let hsb = writer.handle_start_bits();
        DwgMergedReader::new(data, dxf, hsb)
    }

    #[test]
    fn test_table_control_roundtrip() {
        let dwg = DwgVersion::AC15;
        let dxf = DxfVersion::AC1015;

        let mut reader = make_reader(dwg, dxf, |w| {
            w.write_bit_long(3);
            w.write_handle(DwgReferenceType::SoftOwnership, 0x10);
            w.write_handle(DwgReferenceType::SoftOwnership, 0x11);
            w.write_handle(DwgReferenceType::SoftOwnership, 0x12);
        });

        let ctrl = read_table_control(&mut reader);
        assert_eq!(ctrl.entry_count, 3);
        assert_eq!(ctrl.entry_handles.len(), 3);
    }

    #[test]
    fn test_block_control_roundtrip() {
        let dwg = DwgVersion::AC15;
        let dxf = DxfVersion::AC1015;

        let mut reader = make_reader(dwg, dxf, |w| {
            w.write_bit_long(1);
            w.write_handle(DwgReferenceType::SoftOwnership, 0x20);
            w.write_handle(DwgReferenceType::HardOwnership, 0xA0); // *Model_Space
            w.write_handle(DwgReferenceType::HardOwnership, 0xA1); // *Paper_Space
        });

        let ctrl = read_block_control(&mut reader);
        assert_eq!(ctrl.entry_count, 1);
        assert_eq!(ctrl.entry_handles.len(), 1);
        assert_eq!(ctrl.model_space_handle, 0xA0);
        assert_eq!(ctrl.paper_space_handle, 0xA1);
    }

    #[test]
    fn test_layer_roundtrip_r2000() {
        let dwg = DwgVersion::AC15;
        let dxf = DxfVersion::AC1015;

        let mut reader = make_reader(dwg, dxf, |w| {
            w.write_variable_text("Layer0");
            w.write_bit(false); // xref_64
            w.write_bit_short(0); // xrefindex+1
            w.write_bit(false); // xref_dep
            // R2000+: packed values (lineweight=0, frozen=0, off=0, locked=0, plottable=1)
            w.write_bit_short(0b10000); // plottable only
            // Color
            w.write_cm_color(&Color::from_index(7));
            // Xref handle
            w.write_handle(DwgReferenceType::HardPointer, 0);
            // Plotstyle handle
            w.write_handle(DwgReferenceType::HardPointer, 0);
            // Linetype handle
            w.write_handle(DwgReferenceType::HardPointer, 0x30);
        });

        let layer = read_layer(&mut reader, dwg, dxf);
        assert_eq!(layer.name, "Layer0");
        assert!(!layer.frozen);
        assert!(!layer.off);
        assert!(layer.plottable);
        assert_eq!(layer.linetype_handle, 0x30);
    }

    #[test]
    fn test_text_style_roundtrip() {
        let dwg = DwgVersion::AC15;
        let dxf = DxfVersion::AC1015;

        let mut reader = make_reader(dwg, dxf, |w| {
            w.write_variable_text("Standard");
            w.write_bit(false); // xref_64
            w.write_bit_short(0); // xrefindex+1
            w.write_bit(false); // xref_dep
            w.write_bit(false); // shape file
            w.write_bit(false); // vertical
            w.write_bit_double(0.0);
            w.write_bit_double(1.0);
            w.write_bit_double(0.0);
            w.write_byte(0);
            w.write_bit_double(0.2);
            w.write_variable_text("txt.shx");
            w.write_variable_text("");
            w.write_handle(DwgReferenceType::HardPointer, 0);
        });

        let style = read_text_style(&mut reader, dwg);
        assert_eq!(style.name, "Standard");
        assert!((style.width_factor - 1.0).abs() < 1e-10);
        assert_eq!(style.font_file, "txt.shx");
    }

    #[test]
    fn test_appid_roundtrip() {
        let dwg = DwgVersion::AC15;
        let dxf = DxfVersion::AC1015;

        let mut reader = make_reader(dwg, dxf, |w| {
            w.write_variable_text("ACAD");
            w.write_bit(false); // xref_64
            w.write_bit_short(0); // xrefindex+1
            w.write_bit(false); // xref_dep
            w.write_byte(0);
            w.write_handle(DwgReferenceType::HardPointer, 0);
        });

        let app = read_appid(&mut reader, dwg);
        assert_eq!(app.name, "ACAD");
    }
}
