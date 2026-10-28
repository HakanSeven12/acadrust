//! Transactional edits of decoded native Arch dimensions.

use super::entity::{rewrite_preview, transaction, ArchEditError, ArchEntityData, ArchKind};
use super::geometry::{point_is_finite, positive};
use super::metrics::{member_frame, near, plan_frame, PlanFrame};
use crate::types::Vector3;

/// Set a door's leaf width and height.
///
/// The supported axis-aligned layout keeps the native insertion frame fixed
/// and updates every cached profile together with the native size pair.
pub fn set_door(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    width: f64,
    height: f64,
) -> Result<(), ArchEditError> {
    set_opening_dimensions(data, graphic_data, ArchKind::Door, width, height)
}

/// Set a window's plan width and height.
pub fn set_window(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    width: f64,
    height: f64,
) -> Result<(), ArchEditError> {
    set_opening_dimensions(data, graphic_data, ArchKind::Window, width, height)
}

/// Set an opening's width along the wall and its height.
pub fn set_opening(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    width: f64,
    height: f64,
) -> Result<(), ArchEditError> {
    set_opening_dimensions(data, graphic_data, ArchKind::Opening, width, height)
}

/// Unsupported: native window-assembly placement has not been decoded.
pub fn set_window_assembly(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::WindowAssembly, min, max)
}

/// Unsupported: linked stair rise, tread, and offset parameters are not decoded.
/// The entity and its graphics stay unchanged.
pub fn set_stair(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    run: f64,
    width: f64,
    height: f64,
) -> Result<(), ArchEditError> {
    let _ = (data, graphic_data);
    if !positive(run) || !positive(width) || !positive(height) {
        return Err(ArchEditError::InvalidValue);
    }
    Err(ArchEditError::UnsupportedLayout)
}

/// Unsupported: native railing placement has not been decoded.
pub fn set_railing(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::Railing, min, max)
}

/// Unsupported: native slab placement has not been decoded.
pub fn set_slab(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::Slab, min, max)
}

/// Unsupported: native roof placement has not been decoded.
pub fn set_roof(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::Roof, min, max)
}

/// Unsupported: native roof-slab placement has not been decoded.
pub fn set_roof_slab(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::RoofSlab, min, max)
}

/// Unsupported: native space placement has not been decoded.
pub fn set_space(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::Space, min, max)
}

/// Unsupported: native curtain-wall placement has not been decoded.
pub fn set_curtain_wall(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::CurtainWall, min, max)
}

/// Unsupported: native column-grid placement has not been decoded.
pub fn set_column_grid(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::ColumnGrid, min, max)
}

/// Set a rectangular member's length along its decoded native placement axis.
/// Cross-section changes are unsupported and leave the entity unchanged.
pub fn set_member(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    length: f64,
    breadth: f64,
    depth: f64,
) -> Result<(), ArchEditError> {
    let Some(size) = super::metrics::member_size(data) else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    if !positive(length) {
        return Err(ArchEditError::InvalidValue);
    }
    if !near(breadth, size.breadth) || !near(depth, size.depth) {
        return Err(ArchEditError::Rejected);
    }
    let Some(frame) = member_frame(data) else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    let Some((min, max)) = member_bounds(data, frame) else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    let Some(body) = super::geometry::native_body_bit(data) else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    if !near(min[0], 0.0)
        || !near(max[0], size.length)
        || !near(min[1], -size.breadth * 0.5)
        || !near(max[1], size.breadth * 0.5)
        || !near(max[2] - min[2], size.depth)
        || !(near(min[2], -size.depth * 0.5) && near(max[2], size.depth * 0.5)
            || near(min[2], -size.depth) && near(max[2], 0.0))
        || !rectangular_member_preview(data, frame, min, max)
    {
        return Err(ArchEditError::Rejected);
    }
    transaction(data, graphic_data, |next, graphics| {
        let Some(raw) = next.raw_dwg_data.as_mut() else {
            return Err(ArchEditError::UnsupportedLayout);
        };
        if !super::metrics::write_member_length(raw, body, size, length) {
            return Err(ArchEditError::Rejected);
        }
        rewrite_preview(next, graphics, |point| {
            let relative = point - frame.origin;
            point + frame.along * (relative.dot(&frame.along) * (length / size.length - 1.0))
        })
    })
}

/// Unsupported: native mass-element placement has not been decoded.
pub fn set_mass_element(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::MassElement, min, max)
}

/// Unsupported: native display-theme placement has not been decoded.
pub fn set_display_theme(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::DisplayTheme, min, max)
}

/// Unsupported: native ceiling-grid placement has not been decoded.
pub fn set_ceiling_grid(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    set_kind_bounds(data, graphic_data, ArchKind::CeilingGrid, min, max)
}

fn set_opening_dimensions(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    kind: ArchKind,
    width: f64,
    height: f64,
) -> Result<(), ArchEditError> {
    if data.kind != kind {
        return Err(ArchEditError::UnsupportedLayout);
    }
    if !positive(width) || !positive(height) {
        return Err(ArchEditError::InvalidValue);
    }
    let size = match kind {
        ArchKind::Door => super::metrics::door_size(data),
        ArchKind::Window => super::metrics::window_size(data),
        ArchKind::Opening => super::metrics::opening_size(data),
        _ => None,
    };
    let Some(size) = size else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    let Some(frame) = plan_frame(data) else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    let Some(bounds) = local_bounds(data, frame) else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    let along_span = bounds[1] - bounds[0];
    let across_span = bounds[3] - bounds[2];
    if !positive(along_span) || !positive(across_span) || !near(bounds[0], 0.0) {
        return Err(ArchEditError::Rejected);
    }
    let simple_rectangle = rectangular_opening_preview(data, frame, size.width);
    let (along_scale, across_scale) = if simple_rectangle {
        (width / size.width, 1.0)
    } else if kind == ArchKind::Door {
        if !near(along_span, size.width + 100.0)
            || !near(across_span, size.width + 120.0)
            || !near(bounds[2], 0.0)
        {
            return Err(ArchEditError::Rejected);
        }
        ((width + 100.0) / along_span, (width + 120.0) / across_span)
    } else {
        if !near(along_span, size.width) {
            return Err(ArchEditError::Rejected);
        }
        (width / size.width, 1.0)
    };
    transaction(data, graphic_data, |next, graphics| {
        let Some(raw) = next.raw_dwg_data.as_mut() else {
            return Err(ArchEditError::UnsupportedLayout);
        };
        if !super::metrics::write_opening_size(raw, size, width, height) {
            return Err(ArchEditError::Rejected);
        }
        rewrite_preview(next, graphics, |point| {
            let relative = point - frame.origin;
            let along = relative.dot(&frame.along);
            let across = relative.dot(&frame.across);
            point
                + frame.along * (along * (along_scale - 1.0))
                + frame.across * (across * (across_scale - 1.0))
        })
    })
}

fn rectangular_opening_preview(data: &ArchEntityData, frame: PlanFrame, width: f64) -> bool {
    let [profile] = data.profiles.as_slice() else {
        return false;
    };
    if profile.len() != 5 || profile.first() != profile.last() {
        return false;
    }
    let expected = [
        (0.0, 0.0),
        (width, 0.0),
        (width, 50.0),
        (0.0, 50.0),
        (0.0, 0.0),
    ];
    profile
        .iter()
        .zip(expected)
        .all(|(point, (along, across))| {
            (*point - (frame.origin + frame.along * along + frame.across * across)).length() <= 1e-6
        })
}

fn set_kind_bounds(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    kind: ArchKind,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    let _ = graphic_data;
    if data.kind != kind {
        return Err(ArchEditError::UnsupportedLayout);
    }
    if !point_is_finite(min) || !point_is_finite(max) {
        return Err(ArchEditError::InvalidValue);
    }
    Err(ArchEditError::UnsupportedLayout)
}

fn member_bounds(
    data: &ArchEntityData,
    frame: super::metrics::MemberFrame,
) -> Option<([f64; 3], [f64; 3])> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for point in data.profiles.iter().flatten() {
        if !point_is_finite(*point) {
            return None;
        }
        let relative = *point - frame.origin;
        for (index, axis) in [frame.along, frame.section_x, frame.section_y]
            .into_iter()
            .enumerate()
        {
            let value = relative.dot(&axis);
            min[index] = min[index].min(value);
            max[index] = max[index].max(value);
        }
    }
    min.iter()
        .chain(max.iter())
        .all(|value| value.is_finite())
        .then_some((min, max))
}

fn rectangular_member_preview(
    data: &ArchEntityData,
    frame: super::metrics::MemberFrame,
    min: [f64; 3],
    max: [f64; 3],
) -> bool {
    let mut corners = [false; 8];
    for point in data.profiles.iter().flatten() {
        let relative = *point - frame.origin;
        let coordinates = [
            relative.dot(&frame.along),
            relative.dot(&frame.section_x),
            relative.dot(&frame.section_y),
        ];
        let mut index = 0;
        for axis in 0..3 {
            if near(coordinates[axis], min[axis]) {
                continue;
            }
            if near(coordinates[axis], max[axis]) {
                index |= 1 << axis;
            } else {
                return false;
            }
        }
        corners[index] = true;
    }
    corners.into_iter().all(|found| found)
}

fn local_bounds(data: &ArchEntityData, frame: PlanFrame) -> Option<[f64; 4]> {
    if frame.along.x.abs() > 1e-6 && frame.along.y.abs() > 1e-6 {
        return None;
    }
    if frame.across.x.abs() > 1e-6 && frame.across.y.abs() > 1e-6 {
        return None;
    }
    let mut points = data.profiles.iter().flatten().copied();
    let first = points.next()?;
    if !point_is_finite(first) {
        return None;
    }
    let relative = first - frame.origin;
    let u = relative.dot(&frame.along);
    let v = relative.dot(&frame.across);
    let (mut min_u, mut max_u, mut min_v, mut max_v) = (u, u, v, v);
    for point in points {
        if !point_is_finite(point) {
            return None;
        }
        let relative = point - frame.origin;
        let (u, v) = (relative.dot(&frame.along), relative.dot(&frame.across));
        min_u = min_u.min(u);
        max_u = max_u.max(u);
        min_v = min_v.min(v);
        max_v = max_v.max(v);
    }
    Some([min_u, max_u, min_v, max_v])
}
