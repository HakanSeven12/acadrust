//! Sizes decoded from validated native Arch record layouts.

use super::geometry::{
    native_body_bit, native_plan_frame, native_record_layout, positive, read_bit_double_at,
    read_encoded_double,
};
use super::{ArchEntityData, ArchKind};
use crate::types::{DxfVersion, Vector3};

#[derive(Clone, Copy)]
pub(crate) struct PlanFrame {
    pub origin: Vector3,
    pub along: Vector3,
    pub across: Vector3,
    pub width_bit: i64,
}

/// Width and height of a door, window, or opening, in drawing units.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OpeningSize {
    pub width: f64,
    pub height: f64,
    pub(crate) width_bit: i64,
    pub(crate) height_bit: i64,
}

/// Run, width, and floor-to-floor height of a stair.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StairSize {
    pub run: f64,
    pub width: f64,
    pub height: f64,
    pub(crate) run_bit: i64,
    pub(crate) width_bit: i64,
    pub(crate) height_bit: i64,
}

/// Length and the two cross-section sizes of a beam or column.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MemberSize {
    pub length: f64,
    pub breadth: f64,
    pub depth: f64,
    pub(crate) length_bit: i64,
    pub(crate) breadth_bit: i64,
    pub(crate) depth_bit: i64,
}

#[derive(Clone, Copy)]
pub(crate) struct MemberFrame {
    pub origin: Vector3,
    pub along: Vector3,
    pub section_x: Vector3,
    pub section_y: Vector3,
}

/// The measured layout is currently supported only in AC1032 Arch records.
pub(crate) fn native_record(data: &ArchEntityData) -> Option<(&[u8], i64)> {
    if data.dwg_source_version != Some(DxfVersion::AC1032) {
        return None;
    }
    if super::kind_of(&data.dxf_name) != Some(data.kind) {
        return None;
    }
    let raw = data.raw_dwg_data.as_deref()?;
    let record = native_record_layout(data)?;
    if super::geometry::decode_profiles(Some(&record.preview)) != data.profiles {
        return None;
    }
    Some((raw, native_body_bit(data)?))
}

pub(crate) fn plan_frame(data: &ArchEntityData) -> Option<PlanFrame> {
    native_record(data)?;
    let schema = match data.kind {
        ArchKind::Door => 558,
        ArchKind::Window => 559,
        ArchKind::Opening => 560,
        _ => return None,
    };
    let frame = native_plan_frame(data, schema)?;
    let values = frame.values;
    let along = Vector3::new(values[3], values[4], values[5]);
    let across = Vector3::new(values[6], values[7], values[8]);
    if along.z.abs() > 1e-9
        || across.z.abs() > 1e-9
        || (along.length() - 1.0).abs() > 1e-6
        || (across.length() - 1.0).abs() > 1e-6
        || along.dot(&across).abs() > 1e-6
    {
        return None;
    }
    Some(PlanFrame {
        origin: Vector3::new(values[0], values[1], values[2]),
        along,
        across,
        width_bit: frame.body.position_in_bits(),
    })
}

/// Door leaf width and height, excluding the insertion elevation and swing clearance.
pub fn door_size(data: &ArchEntityData) -> Option<OpeningSize> {
    opening_pair(data, ArchKind::Door)
}

/// Opening width along the wall, and its height.
pub fn opening_size(data: &ArchEntityData) -> Option<OpeningSize> {
    opening_pair(data, ArchKind::Opening)
}

/// Window width and height. The insertion-point Z coordinate is the sill elevation.
pub fn window_size(data: &ArchEntityData) -> Option<OpeningSize> {
    opening_pair(data, ArchKind::Window)
}

fn opening_pair(data: &ArchEntityData, kind: ArchKind) -> Option<OpeningSize> {
    if data.kind != kind {
        return None;
    }
    let (raw, _) = native_record(data)?;
    let frame = plan_frame(data)?;
    let width_bit = frame.width_bit;
    let height_bit = width_bit + 66;
    let width = read_bit_double_at(raw, width_bit).filter(|v| positive(*v))?;
    let height = read_bit_double_at(raw, height_bit).filter(|v| positive(*v))?;
    if data.profiles.is_empty()
        || data
            .profiles
            .iter()
            .flatten()
            .any(|p| !super::geometry::point_is_finite(*p))
    {
        return None;
    }
    Some(OpeningSize {
        width,
        height,
        width_bit,
        height_bit,
    })
}

/// Dimensions of the measured straight-stair layout.
pub fn stair_size(data: &ArchEntityData) -> Option<StairSize> {
    if data.kind != ArchKind::Stair {
        return None;
    }
    let (raw, body) = native_record(data)?;
    let height_bit = body + 350;
    let run_bit = body + 482;
    let width_bit = body + 3040;
    let height = read_bit_double_at(raw, height_bit).filter(|v| positive(*v))?;
    let run = read_bit_double_at(raw, run_bit).filter(|v| positive(*v))?;
    let width = read_bit_double_at(raw, width_bit).filter(|v| positive(*v))?;
    for (relative, expected) in [
        (416, height),
        (3812, run),
        (4224, height),
        (4360, run),
        (4464, width),
    ] {
        if !near(read_bit_double_at(raw, body + relative)?, expected) {
            return None;
        }
    }
    Some(StairSize {
        run,
        width,
        height,
        run_bit,
        width_bit,
        height_bit,
    })
}

/// Dimensions of the measured rectangular structural-member layout.
pub fn member_size(data: &ArchEntityData) -> Option<MemberSize> {
    if data.kind != ArchKind::StructuralMember {
        return None;
    }
    let (raw, body) = native_record(data)?;
    let length_bit = body + 232;
    let breadth_bit = body + 304;
    let depth_bit = body + 376;
    let length = read_bit_double_at(raw, length_bit).filter(|v| positive(*v))?;
    let breadth = read_bit_double_at(raw, breadth_bit).filter(|v| positive(*v))?;
    let depth = read_bit_double_at(raw, depth_bit).filter(|v| positive(*v))?;
    if !near(read_bit_double_at(raw, body + 934)?, length)
        || !near(read_bit_double_at(raw, body + 100)?, -breadth * 0.5)
    {
        return None;
    }
    let local_y = read_bit_double_at(raw, body + 166)?;
    if !near(local_y, -depth * 0.5) && !near(local_y, -depth) {
        return None;
    }
    Some(MemberSize {
        length,
        breadth,
        depth,
        length_bit,
        breadth_bit,
        depth_bit,
    })
}

/// Placement frame stored immediately after the member's native origin.
pub(crate) fn member_frame(data: &ArchEntityData) -> Option<MemberFrame> {
    let (raw, body) = native_record(data)?;
    let mut bit = body + 453;
    let mut values = [0.0; 12];
    for value in &mut values {
        let (decoded, next) = read_encoded_double(raw, bit)?;
        *value = decoded;
        bit = next;
    }
    let along = Vector3::new(values[3], values[4], values[5]);
    let section_x = Vector3::new(values[6], values[7], values[8]);
    let section_y = Vector3::new(values[9], values[10], values[11]);
    let axes = [along, section_x, section_y];
    if axes.iter().any(|axis| (axis.length() - 1.0).abs() > 1e-6)
        || along.dot(&section_x).abs() > 1e-6
        || along.dot(&section_y).abs() > 1e-6
        || section_x.dot(&section_y).abs() > 1e-6
        || (section_x.cross(&section_y).dot(&along) - 1.0).abs() > 1e-6
    {
        return None;
    }
    Some(MemberFrame {
        origin: Vector3::new(values[0], values[1], values[2]),
        along,
        section_x,
        section_y,
    })
}

pub(crate) fn near(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-6 * left.abs().max(right.abs()).max(1.0)
}

pub(crate) fn write_opening_size(
    raw: &mut Vec<u8>,
    size: OpeningSize,
    width: f64,
    height: f64,
) -> bool {
    super::geometry::write_bit_doubles_at(
        raw,
        &[(size.width_bit, width), (size.height_bit, height)],
    )
}

pub(crate) fn write_member_length(
    raw: &mut Vec<u8>,
    body: i64,
    size: MemberSize,
    length: f64,
) -> bool {
    super::geometry::write_bit_doubles_at(raw, &[(size.length_bit, length), (body + 934, length)])
}
