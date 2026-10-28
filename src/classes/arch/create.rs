//! Native Arch constructors encoded from parameters and style defaults.

use super::{ArchEntityData, ArchKind, ArchObject};
use crate::classes::{DxfClass, ProxyFlags};
use crate::entities::{
    EntityCommon, EntityType, ExtendedEntity, ExtendedEntityData, ProxyGraphicRecord, ProxyGraphics,
};
use crate::error::DxfError;
use crate::io::dwg::dwg_reference_type::DwgReferenceType;
use crate::io::dwg::dwg_stream_writers::{object_writer::DwgObjectWriter, DwgMergedWriter};
use crate::objects::{Dictionary, ObjectType};
use crate::tables::{Layer, TableEntry};
use crate::types::{Color, DxfVersion, Handle, LineWeight, Vector3};
use crate::CadDocument;

const APPLICATION: &str = "AecArchBase80|Product Desc: Autodesk Architectural Desktop 2007 |Company: Autodesk,Inc. |WEB Address: www.autodesk.com";

/// Create one straight, centered wall in the world XY plane.
///
/// The baseline must have positive length; dimensions must be finite and positive.
/// `layer` selects the entity layer. `None` uses ACAD Arch's `A-Wall`.
pub fn wall(
    start: Vector3,
    end: Vector3,
    width: f64,
    height: f64,
    layer: Option<&str>,
) -> Result<CadDocument, DxfError> {
    validate_size(width, height)?;
    if !finite(start) || !finite(end) || start.z != 0.0 || end.z != 0.0 {
        return Err(invalid(
            "wall baseline must be finite and in the world XY plane",
        ));
    }
    let length = (end - start).length();
    if !length.is_finite() || length <= 0.0 {
        return Err(invalid("wall baseline must have positive finite length"));
    }
    let direction = (end - start) / length;
    let profile = super::centered_wall_profile(start, end, width)
        .ok_or_else(|| invalid("wall dimensions are too small for a supported profile"))?;
    construct(
        ArchKind::Wall,
        start,
        direction,
        width,
        height,
        length,
        profile,
        layer,
    )
}

/// Create one standalone, unanchored native door at the origin.
///
/// Dimensions are in drawing units and must be finite and positive.
/// `layer` selects the entity layer. `None` uses ACAD Arch's `A-Door`.
pub fn door(width: f64, height: f64, layer: Option<&str>) -> Result<CadDocument, DxfError> {
    validate_size(width, height)?;
    construct(
        ArchKind::Door,
        Vector3::ZERO,
        Vector3::UNIT_X,
        width,
        height,
        0.0,
        rectangle(width + 100.0, width + 120.0),
        layer,
    )
}

/// Create one standalone, unanchored native window at the origin.
///
/// Dimensions are in drawing units and must be finite and positive.
/// `layer` selects the entity layer. `None` uses ACAD Arch's `A-Glaz`.
pub fn window(width: f64, height: f64, layer: Option<&str>) -> Result<CadDocument, DxfError> {
    validate_size(width, height)?;
    construct(
        ArchKind::Window,
        Vector3::ZERO,
        Vector3::UNIT_X,
        width,
        height,
        0.0,
        rectangle(width, 50.0),
        layer,
    )
}

/// ACAD Arch layers observed on the sample drawings.
struct ArchLayer {
    name: &'static str,
    color: i16,
    line_weight: i16,
}

const ARCH_LAYERS: &[ArchLayer] = &[
    ArchLayer {
        name: "A-Wall",
        color: 113,
        line_weight: 50,
    },
    ArchLayer {
        name: "A-Door",
        color: 31,
        line_weight: 25,
    },
    ArchLayer {
        name: "A-Glaz",
        color: 151,
        line_weight: 25,
    },
    ArchLayer {
        name: "A-Wall-Open",
        color: 31,
        line_weight: 25,
    },
    ArchLayer {
        name: "A-Flor-Strs",
        color: 31,
        line_weight: 25,
    },
    ArchLayer {
        name: "A-Flor-Hral",
        color: 11,
        line_weight: 25,
    },
    ArchLayer {
        name: "A-Glaz-Assm",
        color: 71,
        line_weight: 25,
    },
    ArchLayer {
        name: "A-Glaz-Curt",
        color: 52,
        line_weight: 35,
    },
    ArchLayer {
        name: "A-Area-Spce",
        color: 32,
        line_weight: 35,
    },
    ArchLayer {
        name: "A-Slab",
        color: 192,
        line_weight: 35,
    },
    ArchLayer {
        name: "A-Roof",
        color: 132,
        line_weight: 35,
    },
    ArchLayer {
        name: "A-Roof-Slab",
        color: 12,
        line_weight: 35,
    },
    ArchLayer {
        name: "S-Beam",
        color: 12,
        line_weight: 50,
    },
    ArchLayer {
        name: "S-Cols",
        color: 52,
        line_weight: 35,
    },
];

fn default_layer(kind: ArchKind) -> Result<&'static ArchLayer, DxfError> {
    let name = match kind {
        ArchKind::Wall => "A-Wall",
        ArchKind::Door => "A-Door",
        ArchKind::Window => "A-Glaz",
        _ => return Err(invalid("no ACAD Arch layer is defined for this class")),
    };
    ARCH_LAYERS
        .iter()
        .find(|layer| layer.name == name)
        .ok_or_else(|| invalid("ACAD Arch layer table is missing a standard layer"))
}

fn ensure_entity_layer(
    document: &mut CadDocument,
    kind: ArchKind,
    requested: Option<&str>,
) -> Result<(String, Handle), DxfError> {
    let standard = default_layer(kind)?;
    let name = match requested {
        None => standard.name.to_string(),
        Some(name) => {
            let name = name.trim();
            if name.is_empty() {
                return Err(invalid("layer name must not be empty"));
            }
            ARCH_LAYERS
                .iter()
                .find(|layer| layer.name.eq_ignore_ascii_case(name))
                .map(|layer| layer.name.to_string())
                .unwrap_or_else(|| name.to_string())
        }
    };
    if let Some(existing) = document.layers.get(&name) {
        document
            .header
            .current_layer_name
            .clone_from(&existing.name);
        document.header.current_layer_handle = existing.handle;
        return Ok((existing.name.clone(), existing.handle));
    }
    let mut layer = Layer::new(&name);
    if let Some(known) = ARCH_LAYERS.iter().find(|layer| layer.name == name) {
        layer.color = Color::from_index(known.color);
        layer.line_weight = LineWeight::from_value(known.line_weight);
    }
    layer.set_handle(document.allocate_handle());
    let handle = layer.handle;
    document
        .layers
        .add(layer)
        .map_err(|error| invalid(&error.to_string()))?;
    document.header.current_layer_name = name.clone();
    document.header.current_layer_handle = handle;
    Ok((name, handle))
}

fn invalid(message: &str) -> DxfError {
    DxfError::Parse(message.into())
}
fn finite(point: Vector3) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.z.is_finite()
}
fn validate_size(width: f64, height: f64) -> Result<(), DxfError> {
    if !width.is_finite() || width <= 0.0 || !height.is_finite() || height <= 0.0 {
        Err(invalid("Arch dimensions must be finite and positive"))
    } else {
        Ok(())
    }
}
fn rectangle(width: f64, depth: f64) -> Vec<Vector3> {
    vec![
        Vector3::ZERO,
        Vector3::new(width, 0.0, 0.0),
        Vector3::new(width, depth, 0.0),
        Vector3::new(0.0, depth, 0.0),
        Vector3::ZERO,
    ]
}

fn construct(
    kind: ArchKind,
    location: Vector3,
    direction: Vector3,
    width: f64,
    height: f64,
    length: f64,
    profile: Vec<Vector3>,
    layer: Option<&str>,
) -> Result<CadDocument, DxfError> {
    let mut document = CadDocument::new();
    document.version = DxfVersion::AC1032;
    let material = add_style(
        &mut document,
        "AEC_MATERIAL_DEFS",
        "AEC_MATERIAL_DEF",
        "AecDbMaterialDef",
        164,
        material_body,
    )?;
    let (style_name, style_cpp, dictionary, schema) = match kind {
        ArchKind::Wall => ("AEC_WALL_STYLE", "AecDbWallStyle", "AEC_WALL_STYLES", 547),
        ArchKind::Door => ("AEC_DOOR_STYLE", "AecDbDoorStyle", "AEC_DOOR_STYLES", 537),
        ArchKind::Window => (
            "AEC_WINDOW_STYLE",
            "AecDbWindowStyle",
            "AEC_WINDOW_STYLES",
            548,
        ),
        _ => unreachable!(),
    };
    let mut cleanup = Handle::NULL;
    let style = if kind == ArchKind::Wall {
        let endcap = add_style(
            &mut document,
            "AEC_ENDCAP_STYLES",
            "AEC_ENDCAP_STYLE",
            "AecDbEndcapStyle",
            538,
            endcap_body,
        )?;
        let frame = add_style(
            &mut document,
            "AEC_OPENING_ENDCAP_STYLES",
            "AEC_FRAME_DEF",
            "AecDbOpeningEndcapStyle",
            674,
            |w| frame_body(w, endcap),
        )?;
        cleanup = add_style(
            &mut document,
            "AEC_CLEANUP_GROUP_DEFS",
            "AEC_CLEANUP_GROUP_DEF",
            "AecDbCleanupGroupDef",
            536,
            cleanup_body,
        )?;
        add_style(
            &mut document,
            dictionary,
            style_name,
            style_cpp,
            schema,
            |w| wall_style_body(w, material, endcap, frame, cleanup),
        )?
    } else {
        add_style(
            &mut document,
            dictionary,
            style_name,
            style_cpp,
            schema,
            |w| opening_style_body(w, kind, material),
        )?
    };
    let (name, cpp, schema) = match kind {
        ArchKind::Wall => ("AEC_WALL", "AecDbWall", 569),
        ArchKind::Door => ("AEC_DOOR", "AecDbDoor", 558),
        ArchKind::Window => ("AEC_WINDOW", "AecDbWindow", 559),
        _ => unreachable!(),
    };
    let type_code = register_class(&mut document, name, cpp, true)?;
    let mut graphics = Vec::new();
    graphics.extend_from_slice(&(profile.len() as u32).to_le_bytes());
    for point in &profile {
        for value in [point.x, point.y, point.z] {
            graphics.extend_from_slice(&value.to_le_bytes());
        }
    }
    let graphic_data = ProxyGraphics {
        records: vec![ProxyGraphicRecord::Unknown {
            record_type: 6,
            data: graphics,
        }],
    }
    .encode();
    let (layer, layer_handle) = ensure_entity_layer(&mut document, kind, layer)?;
    let common = EntityCommon {
        handle: document.allocate_handle(),
        owner_handle: document.header.model_space_block_handle,
        layer,
        layer_handle: Some(layer_handle),
        graphic_data,
        ..EntityCommon::default()
    };
    let mut writer = DwgObjectWriter::new(&document)?;
    let (raw, handle_bits) = writer.encode_architecture_entity(type_code, &common, |w| {
        entity_prefix(w, schema, location, direction);
        match kind {
            ArchKind::Wall => wall_body(w, width, height, length, style, cleanup),
            ArchKind::Door | ArchKind::Window => opening_body(w, kind, width, height, style),
            _ => unreachable!(),
        }
        entity_tail(w, kind, common.handle);
    });
    let mut data = ArchEntityData::from_parts(name, cpp, common.graphic_data.as_deref());
    data.dwg_type_code = type_code;
    data.dwg_handle_bits = handle_bits;
    data.dwg_source_version = Some(DxfVersion::AC1032);
    data.raw_dwg_data = Some(raw);
    data.decode_fields();
    document.add_entity(EntityType::Extended(Box::new(ExtendedEntity {
        common,
        data: ExtendedEntityData::Arch(data),
    })))?;
    Ok(document)
}

fn register_class(
    document: &mut CadDocument,
    name: &str,
    cpp: &str,
    entity: bool,
) -> Result<i16, DxfError> {
    let mut class = if entity {
        DxfClass::new_entity(name, cpp)
    } else {
        DxfClass::new(name, cpp)
    };
    class.application_name = if name == "AEC_MATERIAL_DEF" {
        APPLICATION.replacen("AecArchBase80", "AecBase80", 1)
    } else {
        APPLICATION.into()
    };
    class.proxy_flags = ProxyFlags(if entity { 893 } else { 129 });
    class.dwg_version = match name {
        "AEC_MATERIAL_DEF" | "AEC_FRAME_DEF" => 25,
        "AEC_CLEANUP_GROUP_DEF" => 23,
        _ => 21,
    };
    if name == "AEC_CLEANUP_GROUP_DEF" {
        class.maintenance_version = 3;
    }
    document.classes.add_or_update(class);
    document
        .classes
        .get_by_name(name)
        .map(|class| class.class_number)
        .ok_or_else(|| invalid("Arch class was not registered"))
}

fn add_style(
    document: &mut CadDocument,
    dictionary_name: &str,
    name: &str,
    cpp: &str,
    schema: i16,
    body: impl FnOnce(&mut DwgMergedWriter),
) -> Result<Handle, DxfError> {
    let type_code = register_class(document, name, cpp, false)?;
    let root = document.header.named_objects_dict_handle;
    let owner = document.allocate_handle();
    let handle = document.allocate_handle();
    let mut dictionary = Dictionary::new();
    dictionary.handle = owner;
    dictionary.owner = root;
    dictionary.reactors.push(root);
    dictionary.add_entry("Standard", handle);
    document
        .objects
        .insert(owner, ObjectType::Dictionary(dictionary));
    let Some(ObjectType::Dictionary(root_dictionary)) = document.objects.get_mut(&root) else {
        return Err(invalid("new drawing has no named objects dictionary"));
    };
    root_dictionary.add_entry(dictionary_name, owner);
    let mut writer = DwgObjectWriter::new(document)?;
    let (raw, handle_bits) = writer.encode_architecture_object(type_code, handle, owner, |w| {
        object_prefix(w, schema);
        body(w);
    });
    let mut object = ArchObject::new(name, handle, owner);
    object.cpp_class_name = cpp.into();
    object.reactors.push(owner);
    object.raw_dwg_data = Some(raw);
    object.raw_dwg_handle_bits = handle_bits;
    object.raw_dwg_version = Some(DxfVersion::AC1032);
    document.objects.insert(handle, ObjectType::Arch(object));
    Ok(handle)
}

fn bs(w: &mut DwgMergedWriter, value: i16) {
    w.write_bit_short(value);
}
fn bd(w: &mut DwgMergedWriter, value: f64) {
    w.write_bit_double(value);
}
// Geometry setters rewrite these constructor fields without resizing the AEC record.
fn editable_bd(w: &mut DwgMergedWriter, value: f64) {
    w.write_2bits(0);
    w.write_raw_double(value);
}
fn marker(w: &mut DwgMergedWriter) {
    bs(w, 80);
}
fn text(w: &mut DwgMergedWriter, value: &str) {
    w.write_variable_text(value);
}
fn native_style_tail(w: &mut DwgMergedWriter) {
    // FileFiler's native 106-bit extension shared by wall, door, and window styles.
    const TAIL: [u8; 14] = [
        0xD2, 0xD5, 0x41, 0x50, 0x40, 0x77, 0x11, 0x50, 0x54, 0x15, 0x09, 0x50, 0x54, 0x00,
    ];
    let mut remaining = 106;
    for byte in TAIL {
        let count = remaining.min(8);
        for shift in (8 - count..8).rev() {
            w.write_bit(byte & (1 << shift) != 0);
        }
        remaining -= count;
    }
    text(w, "");
    text(w, "");
}
fn pointer(w: &mut DwgMergedWriter, handle: Handle) {
    w.write_handle(DwgReferenceType::HardPointer, handle.value());
}
fn empty_group(w: &mut DwgMergedWriter, count: i32) {
    marker(w);
    text(w, "");
    marker(w);
    w.write_bit_long(count);
}
fn class_tag(w: &mut DwgMergedWriter, schema: i16) {
    w.write_bit(true);
    w.write_bit(true);
    bs(w, schema);
}
fn object_prefix(w: &mut DwgMergedWriter, schema: i16) {
    marker(w);
    class_tag(w, schema);
    marker(w);
    text(w, "");
    marker(w);
    text(w, "");
    w.write_bit(false);
    w.write_byte(0);
    marker(w);
}
fn object_tail(w: &mut DwgMergedWriter) {
    // Native records end on the second marker. A following zero is only what a
    // filer returns after the object bits are already exhausted.
    marker(w);
    marker(w);
}
fn material_body(w: &mut DwgMergedWriter) {
    w.write_bit_long(0);
    w.write_bit(false);
    object_tail(w);
}
fn cleanup_body(w: &mut DwgMergedWriter) {
    bs(w, 2);
    w.write_bit(false);
    object_tail(w);
}
fn frame_body(w: &mut DwgMergedWriter, endcap: Handle) {
    for _ in 0..4 {
        pointer(w, endcap);
    }
    w.write_bit(false);
    object_tail(w);
}
fn endcap_body(w: &mut DwgMergedWriter) {
    bd(w, 0.0);
    empty_group(w, 1);
    class_tag(w, 30);
    marker(w);
    text(w, "");
    marker(w);
    marker(w);
    text(w, "");
    marker(w);
    w.write_bit_long(1);
    class_tag(w, 33);
    marker(w);
    text(w, "");
    marker(w);
    bs(w, 8);
    w.write_byte(0);
    w.write_bit(false);
    bs(w, 1);
    w.write_bit(false);
    w.write_bit(true);
    for value in [0.0, 0.0, 0.0, 1.0] {
        bd(w, value);
    }
    w.write_bit(false);
    marker(w);
    for value in [1, 8, 8, 0] {
        w.write_bit_long(value);
    }
    w.write_bit(false);
    object_tail(w);
}
fn opening_style_body(w: &mut DwgMergedWriter, kind: ArchKind, material: Handle) {
    w.write_bit(false);
    for value in [
        2.0,
        5.0,
        0.5,
        1.5,
        if kind == ArchKind::Door { 2.0 } else { 0.25 },
    ] {
        bd(w, value);
    }
    bs(w, 1);
    bs(w, 0);
    pointer(w, Handle::NULL);
    for _ in 0..if kind == ArchKind::Door { 5 } else { 4 } {
        pointer(w, material);
    }
    if kind == ArchKind::Door {
        bd(w, 0.25);
    } else {
        w.write_bit(false);
        w.write_bit(true);
        bd(w, 0.0);
    }
    native_style_tail(w);
}
fn wall_style_body(
    w: &mut DwgMergedWriter,
    material: Handle,
    endcap: Handle,
    frame: Handle,
    cleanup: Handle,
) {
    pointer(w, endcap);
    for _ in 0..4 {
        pointer(w, frame);
    }
    pointer(w, material);
    empty_group(w, 1);
    class_tag(w, 652);
    marker(w);
    text(w, "");
    marker(w);
    text(w, "Unnamed");
    bs(w, 1);
    class_tag(w, 653);
    marker(w);
    text(w, "");
    marker(w);
    marker(w);
    text(w, "");
    marker(w);
    bd(w, 0.0);
    bs(w, 3);
    marker(w);
    text(w, "");
    marker(w);
    bd(w, 0.0);
    bs(w, 2);
    marker(w);
    text(w, "");
    marker(w);
    bd(w, 0.0);
    bd(w, 0.0);
    bs(w, 0);
    w.write_bit(true);
    marker(w);
    text(w, "");
    marker(w);
    bd(w, 0.0);
    bd(w, -0.5);
    bs(w, 3);
    w.write_bit(true);
    bs(w, 100);
    pointer(w, material);
    bs(w, 1);
    w.write_byte(5);
    bs(w, 0);
    for value in [6.0, 96.0, 0.0, 0.0, 0.0] {
        bd(w, value);
    }
    w.write_bit(true);
    pointer(w, cleanup);
    w.write_byte(3);
    w.write_bit(true);
    w.write_bit(false);
    native_style_tail(w);
}
fn editable_vector(w: &mut DwgMergedWriter, value: Vector3) {
    for component in [value.x, value.y, value.z] {
        editable_bd(w, component);
    }
}

fn entity_prefix(w: &mut DwgMergedWriter, schema: i16, location: Vector3, direction: Vector3) {
    marker(w);
    class_tag(w, schema);
    marker(w);
    text(w, "");
    marker(w);
    bs(w, 0);
    w.write_byte(1);
    w.write_bit(false);
    marker(w);
    text(w, "");
    for vector in [
        location,
        direction,
        Vector3::new(-direction.y, direction.x, 0.0),
        Vector3::UNIT_Z,
    ] {
        editable_vector(w, vector);
    }
    text(w, "");
    w.write_handle(DwgReferenceType::HardOwnership, 0);
    marker(w);
}
fn wall_body(
    w: &mut DwgMergedWriter,
    width: f64,
    height: f64,
    length: f64,
    style: Handle,
    cleanup: Handle,
) {
    pointer(w, style);
    pointer(w, cleanup);
    editable_bd(w, height);
    bd(w, -1.0);
    editable_bd(w, width);
    bd(w, 12.0);
    w.write_bit(false);
    w.write_bit(false);
    for _ in 0..5 {
        empty_group(w, 0);
    }
    bs(w, 2);
    empty_group(w, 0);
    marker(w);
    editable_bd(w, length);
}
fn opening_body(w: &mut DwgMergedWriter, kind: ArchKind, width: f64, height: f64, style: Handle) {
    editable_bd(w, width);
    editable_bd(w, height);
    bd(w, 0.0);
    marker(w);
    bd(w, 50.0);
    bd(w, 0.0);
    bs(w, 0);
    marker(w);
    pointer(w, style);
    if kind == ArchKind::Window {
        w.write_bit(false);
        bs(w, 0);
        bd(w, 18.0);
        bd(w, 18.0);
        bd(w, std::f64::consts::FRAC_PI_2);
        w.write_bit(true);
    }
}
fn entity_tail(w: &mut DwgMergedWriter, kind: ArchKind, handle: Handle) {
    class_tag(w, 45);
    empty_group(w, 1);
    class_tag(w, 196);
    marker(w);
    text(w, "");
    marker(w);
    marker(w);
    bs(w, 2);
    w.write_handle(DwgReferenceType::SoftPointer, handle.value());
    for _ in 0..if kind == ArchKind::Wall { 2 } else { 4 } {
        marker(w);
    }
}
