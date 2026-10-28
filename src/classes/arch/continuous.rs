//! A continuous wall is a chain of straight `AEC_WALL` segments that share endpoints.
//!
//! `COUNTINIOUS_IRREGULAR_WALL.dwg` is three such runs. Each run keeps its own
//! baseline, thickness, and height. The drawable outline is the mitered ribbon
//! around that polyline, not the axis-aligned preview stored in the file.

use super::entity::{set_wall, ArchKind, StraightWallGeometry};
use super::geometry::{point_is_finite, positive};
use crate::entities::{EntityType, ExtendedEntityData};
use crate::types::{Handle, Vector3};
use crate::CadDocument;

const ENDPOINT_TOLERANCE: f64 = 1e-6;

/// One connected run of wall segments, in baseline order.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContinuousWall {
    /// Segment entity handles, one per edge of [`points`](Self::points).
    pub handles: Vec<Handle>,
    /// Baseline vertices. `points.len() == handles.len() + 1`.
    pub points: Vec<Vector3>,
    pub width: f64,
    pub height: f64,
}

/// Walls in `document` grouped into continuous chains.
pub fn continuous_walls(document: &CadDocument) -> Vec<ContinuousWall> {
    let mut segments = Vec::new();
    for entity in document.entities() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &entity.data else {
            continue;
        };
        if data.kind != ArchKind::Wall {
            continue;
        }
        let Some(geometry) = data.straight_wall_geometry() else {
            continue;
        };
        segments.push((entity.common.handle, geometry));
    }
    let mut unused = vec![true; segments.len()];
    let mut chains = Vec::new();
    for index in 0..segments.len() {
        if !unused[index] {
            continue;
        }
        unused[index] = false;
        let mut handles = vec![segments[index].0];
        let mut points = vec![segments[index].1.start, segments[index].1.end];
        let width = segments[index].1.width;
        let height = segments[index].1.height;
        loop {
            let mut extended = false;
            for (candidate, free) in segments.iter().zip(unused.iter_mut()) {
                if !*free {
                    continue;
                }
                if !super::metrics::near(candidate.1.width, width)
                    || !super::metrics::near(candidate.1.height, height)
                {
                    continue;
                }
                if let Some(joined) = join(&mut points, &candidate.1) {
                    *free = false;
                    if joined {
                        handles.push(candidate.0);
                    } else {
                        handles.insert(0, candidate.0);
                    }
                    extended = true;
                    break;
                }
            }
            if !extended {
                break;
            }
        }
        chains.push(ContinuousWall {
            handles,
            points,
            width,
            height,
        });
    }
    chains
}

/// Replace every segment of `chain` so the baseline becomes `points`.
///
/// `points` must contain one more vertex than the chain has segments. Width and
/// height are written into each segment.
pub fn set_continuous_wall(
    document: &mut CadDocument,
    chain: &ContinuousWall,
    points: &[Vector3],
    width: f64,
    height: f64,
) -> Result<(), super::entity::ArchEditError> {
    if !positive(width) || !positive(height) {
        return Err(super::entity::ArchEditError::InvalidValue);
    }
    if points.len() != chain.handles.len() + 1
        || points.len() < 2
        || points
            .iter()
            .any(|point| !point_is_finite(*point) || point.z.abs() > 1e-12)
        || points
            .windows(2)
            .any(|pair| !positive((pair[1] - pair[0]).length()))
    {
        return Err(super::entity::ArchEditError::InvalidValue);
    }
    let mut changed = Vec::with_capacity(chain.handles.len());
    for (index, handle) in chain.handles.iter().enumerate() {
        if chain.handles[..index].contains(handle) {
            return Err(super::entity::ArchEditError::Rejected);
        }
        let Some(entity) = document.get_entity(*handle) else {
            return Err(super::entity::ArchEditError::UnsupportedLayout);
        };
        let mut entity = entity.clone();
        let EntityType::Extended(extended) = &mut entity else {
            return Err(super::entity::ArchEditError::UnsupportedLayout);
        };
        let ExtendedEntityData::Arch(data) = &mut extended.data else {
            return Err(super::entity::ArchEditError::UnsupportedLayout);
        };
        set_wall(
            data,
            &mut extended.common.graphic_data,
            points[index],
            points[index + 1],
            width,
            height,
        )?;
        changed.push((*handle, entity));
    }
    for (handle, entity) in changed {
        *document
            .get_entity_mut(handle)
            .expect("preflight checked every handle") = entity;
    }
    Ok(())
}

/// Closed mitered outline of a center-justified wall chain.
pub fn continuous_wall_outline(points: &[Vector3], width: f64) -> Option<Vec<Vector3>> {
    if points.len() < 2
        || !positive(width)
        || points
            .iter()
            .any(|point| !point_is_finite(*point) || point.z.abs() > 1e-12)
    {
        return None;
    }
    let half = width * 0.5;
    let mut left = Vec::with_capacity(points.len());
    let mut right = Vec::with_capacity(points.len());
    for index in 0..points.len() {
        let (left_point, right_point) = offset_vertex(points, index, half)?;
        left.push(left_point);
        right.push(right_point);
    }
    let mut outline = left;
    outline.extend(right.into_iter().rev());
    outline.push(outline[0]);
    outline
        .iter()
        .all(|point| point_is_finite(*point))
        .then_some(outline)
}

fn join(points: &mut Vec<Vector3>, segment: &StraightWallGeometry) -> Option<bool> {
    let end = *points.last()?;
    let start = *points.first()?;
    if near(end, segment.start) {
        points.push(segment.end);
        Some(true)
    } else if near(end, segment.end) {
        points.push(segment.start);
        Some(true)
    } else if near(start, segment.end) {
        points.insert(0, segment.start);
        Some(false)
    } else if near(start, segment.start) {
        points.insert(0, segment.end);
        Some(false)
    } else {
        None
    }
}

fn near(left: Vector3, right: Vector3) -> bool {
    (left - right).length() <= ENDPOINT_TOLERANCE
}

fn unit(from: Vector3, to: Vector3) -> Option<Vector3> {
    let edge = to - from;
    let length = edge.length();
    positive(length).then_some(edge * (1.0 / length))
}

fn left_normal(direction: Vector3) -> Vector3 {
    Vector3::new(-direction.y, direction.x, 0.0)
}

fn offset_vertex(points: &[Vector3], index: usize, half: f64) -> Option<(Vector3, Vector3)> {
    if index == 0 {
        let direction = unit(points[0], points[1])?;
        let normal = left_normal(direction) * half;
        return Some((points[0] + normal, points[0] - normal));
    }
    if index + 1 == points.len() {
        let direction = unit(points[index - 1], points[index])?;
        let normal = left_normal(direction) * half;
        return Some((points[index] + normal, points[index] - normal));
    }
    let incoming = unit(points[index - 1], points[index])?;
    let outgoing = unit(points[index], points[index + 1])?;
    let point = points[index];
    let left_in = left_normal(incoming) * half;
    let left_out = left_normal(outgoing) * half;
    let left = line_intersect(
        point + left_in,
        point + left_in + incoming,
        point + left_out,
        point + left_out + outgoing,
    )
    .unwrap_or(point + left_in);
    let right = line_intersect(
        point - left_in,
        point - left_in + incoming,
        point - left_out,
        point - left_out + outgoing,
    )
    .unwrap_or(point - left_in);
    Some((left, right))
}

fn line_intersect(
    origin: Vector3,
    through: Vector3,
    other: Vector3,
    other_through: Vector3,
) -> Option<Vector3> {
    let first = through - origin;
    let second = other_through - other;
    let cross = first.x * second.y - first.y * second.x;
    if cross.abs() < 1e-9 {
        return None;
    }
    let delta = other - origin;
    let parameter = (delta.x * second.y - delta.y * second.x) / cross;
    Some(origin + first * parameter)
}
