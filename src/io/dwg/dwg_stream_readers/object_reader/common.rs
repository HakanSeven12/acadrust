//! Object type constants shared between reader and writer.
//!
//! These match ACadSharp's fixed type codes (ODA spec table 2.12).
//! Fixed types 0–82 use literal codes; non-fixed types use
//! their DXF class number (500+).

// ── Fixed entity types (graphical) ──────────────────────────────────

pub const OBJ_UNUSED: i16 = 0;
pub const OBJ_TEXT: i16 = 1;
pub const OBJ_ATTRIB: i16 = 2;
pub const OBJ_ATTDEF: i16 = 3;
pub const OBJ_BLOCK: i16 = 4;
pub const OBJ_ENDBLK: i16 = 5;
pub const OBJ_SEQEND: i16 = 6;
pub const OBJ_INSERT: i16 = 7;
pub const OBJ_MINSERT: i16 = 8;
// 9 = skipped
pub const OBJ_VERTEX_2D: i16 = 10;
pub const OBJ_VERTEX_3D: i16 = 11;
pub const OBJ_VERTEX_MESH: i16 = 12;
pub const OBJ_VERTEX_PFACE: i16 = 13;
pub const OBJ_VERTEX_PFACE_FACE: i16 = 14;
pub const OBJ_POLYLINE_2D: i16 = 15;
pub const OBJ_POLYLINE_3D: i16 = 16;
pub const OBJ_ARC: i16 = 17;
pub const OBJ_CIRCLE: i16 = 18;
pub const OBJ_LINE: i16 = 19;
pub const OBJ_DIMENSION_ORDINATE: i16 = 20;
pub const OBJ_DIMENSION_LINEAR: i16 = 21;
pub const OBJ_DIMENSION_ALIGNED: i16 = 22;
pub const OBJ_DIMENSION_ANG_3PT: i16 = 23;
pub const OBJ_DIMENSION_ANG_2LN: i16 = 24;
pub const OBJ_DIMENSION_RADIUS: i16 = 25;
pub const OBJ_DIMENSION_DIAMETER: i16 = 26;
pub const OBJ_POINT: i16 = 27;
pub const OBJ_3DFACE: i16 = 28;
pub const OBJ_POLYLINE_PFACE: i16 = 29;
pub const OBJ_POLYLINE_MESH: i16 = 30;
pub const OBJ_SOLID: i16 = 31;
pub const OBJ_TRACE: i16 = 32;
pub const OBJ_SHAPE: i16 = 33;
pub const OBJ_VIEWPORT: i16 = 34;
pub const OBJ_ELLIPSE: i16 = 35;
pub const OBJ_SPLINE: i16 = 36;
pub const OBJ_REGION: i16 = 37;
pub const OBJ_3DSOLID: i16 = 38;
pub const OBJ_BODY: i16 = 39;
pub const OBJ_RAY: i16 = 40;
pub const OBJ_XLINE: i16 = 41;
pub const OBJ_DICTIONARY: i16 = 42;
pub const OBJ_OLEFRAME: i16 = 43;
pub const OBJ_MTEXT: i16 = 44;
pub const OBJ_LEADER: i16 = 45;
pub const OBJ_TOLERANCE: i16 = 46;
pub const OBJ_MLINE: i16 = 47;

// ── Table control / table entry types ───────────────────────────────

pub const OBJ_BLOCK_CONTROL: i16 = 48;
pub const OBJ_BLOCK_HEADER: i16 = 49;
pub const OBJ_LAYER_CONTROL: i16 = 50;
pub const OBJ_LAYER: i16 = 51;
pub const OBJ_STYLE_CONTROL: i16 = 52;
pub const OBJ_STYLE: i16 = 53;
// 54-55 skipped
pub const OBJ_LTYPE_CONTROL: i16 = 56;
pub const OBJ_LTYPE: i16 = 57;
// 58-59 skipped
pub const OBJ_VIEW_CONTROL: i16 = 60;
pub const OBJ_VIEW: i16 = 61;
pub const OBJ_UCS_CONTROL: i16 = 62;
pub const OBJ_UCS: i16 = 63;
pub const OBJ_VPORT_CONTROL: i16 = 64;
pub const OBJ_VPORT: i16 = 65;
pub const OBJ_APPID_CONTROL: i16 = 66;
pub const OBJ_APPID: i16 = 67;
pub const OBJ_DIMSTYLE_CONTROL: i16 = 68;
pub const OBJ_DIMSTYLE: i16 = 69;
pub const OBJ_VPENT_HDR_CONTROL: i16 = 70;
pub const OBJ_VPENT_HDR: i16 = 71;

// ── Non-graphical objects ────────────────────────────────────────────

pub const OBJ_GROUP: i16 = 72;
pub const OBJ_MLINESTYLE: i16 = 73;
pub const OBJ_OLE2FRAME: i16 = 74;

// ── Non-fixed type codes (use class number, 500+) ───────────────────

pub const OBJ_LAYOUT: i16 = 0x52;          // 82
pub const OBJ_DICTIONARYVAR: i16 = 0x68;   // 104
pub const OBJ_TABLECONTENT: i16 = 0x69;     // 105
pub const OBJ_TABLESTYLE: i16 = 0x6A;       // 106
pub const OBJ_HATCH: i16 = 0x6C;           // 108
pub const OBJ_DICTIONARYWDFLT: i16 = 0x6D; // 109
pub const OBJ_PLACEHOLDER: i16 = 0x6E;     // 110
pub const OBJ_LWPOLYLINE: i16 = 0x6F;      // 111
pub const OBJ_IMAGE: i16 = 0x65;           // 101
pub const OBJ_XRECORD: i16 = 0x70;         // 112
pub const OBJ_MULTILEADER: i16 = 0x79;     // 121
pub const OBJ_MLEADERSTYLE: i16 = 0x7B;    // 123

/// Returns true if the type code is a graphical entity (not a table / object).
pub fn is_entity_type(type_code: i16) -> bool {
    // Entities are types 1–47, plus 74 (OLE2FRAME), plus non-fixed entity types ≥500
    matches!(type_code, 1..=47 | 74) || type_code >= 500
}

/// Returns true if the type code is a table control or entry.
pub fn is_table_type(type_code: i16) -> bool {
    matches!(type_code, 48..=71)
}
