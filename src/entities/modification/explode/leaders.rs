use super::*;

pub(super) fn explode_leader(leader: &Leader) -> Vec<EntityType> {
    let n = leader.vertices.len();
    if n < 2 {
        return Vec::new();
    }
    let size = if leader.arrow_size > 1e-9 {
        leader.arrow_size
    } else {
        0.18
    };
    let mut path = leader.vertices.clone();
    let mut result = Vec::new();
    if leader.arrow_enabled {
        let arrow = dimension_arrow(path[0], path[1], size, &leader.common, leader.normal);
        let direction = (path[1] - path[0]).normalize();
        if direction.length_squared() > 1e-20 {
            path[0] = path[0] + direction * size;
        }
        result.push(arrow);
    }
    if path.len() < 2 {
        return result;
    }
    match leader.path_type {
        LeaderPathType::Spline if path.len() >= 3 => {
            let mut spline = Spline::from_control_points((path.len().min(4) - 1) as i32, path);
            spline.common = inherit_common(&leader.common);
            spline.normal = leader.normal;
            result.push(EntityType::Spline(spline));
        }
        _ => {
            let mut polyline = LwPolyline::new();
            polyline.common = inherit_common(&leader.common);
            polyline.normal = leader.normal;
            for point in path {
                polyline.add_point(Vector2::new(point.x, point.y));
            }
            result.push(EntityType::LwPolyline(polyline));
        }
    }
    result
}

pub(super) fn flatten_leader(leader: &Leader) -> Vec<EntityType> {
    leader
        .vertices
        .windows(2)
        .map(|pair| line_entity(pair[0], pair[1], 0.0, leader.normal, &leader.common))
        .collect()
}

/// Length-to-half-width ratio of ACAD's default `_ClosedFilled` arrowhead:
/// a unit-long triangle whose base spans `[-1/6, 1/6]`.
const CLOSED_FILLED_HALF_WIDTH: f64 = 1.0 / 6.0;

/// Whether an arrowhead handle names a real block. A null handle is how a
/// file says "no block": the default `_ClosedFilled` arrowhead.
pub(super) fn names_arrow_block(handle: Option<Handle>) -> bool {
    handle.is_some_and(|handle| handle != Handle::NULL)
}

/// The arrowhead block a leader line uses: its own when it overrides the
/// entity's, the entity's otherwise.
pub(super) fn line_arrowhead_handle(ml: &MultiLeader, line: &LeaderLine) -> Option<Handle> {
    if line
        .override_flags
        .contains(LeaderLinePropertyOverrideFlags::ARROWHEAD)
    {
        line.arrowhead_handle
    } else {
        ml.arrowhead_handle
    }
}

/// The arrowhead size a leader line uses, already scaled by the annotation.
pub(super) fn line_arrowhead_size(ml: &MultiLeader, line: &LeaderLine) -> f64 {
    if line
        .override_flags
        .contains(LeaderLinePropertyOverrideFlags::ARROWHEAD_SIZE)
    {
        line.arrowhead_size
    } else {
        ml.context.arrowhead_size
    }
}

/// A leader line's full path: its own vertices, arrow tip first, then the
/// root's connection point, where the landing starts. ACAD stores that last
/// point on the root only, so a straight leader is a single-vertex line.
pub(super) fn leader_line_path(root: &LeaderRoot, line: &LeaderLine) -> Vec<Vector3> {
    let mut path = line.points.clone();

    if path
        .last()
        .is_some_and(|last| last.distance(&root.connection_point) > f64::EPSILON)
    {
        path.push(root.connection_point);
    }

    path
}

/// The default `_ClosedFilled` arrowhead at the start of `path`, pointing
/// away from the second vertex, and the point where the shaft now starts.
///
/// No arrowhead when the size is not positive or when the first segment is
/// shorter than the arrow itself, which is also what keeps the shaft from
/// being trimmed past its own end.
pub(super) fn closed_filled_arrowhead(
    path: &[Vector3],
    size: f64,
    common: &EntityCommon,
) -> Option<(EntityType, Vector3)> {
    let (tip, next) = (*path.first()?, *path.get(1)?);
    let first_segment = tip - next;
    let length = first_segment.length();

    if size <= 0.0 || length < size {
        return None;
    }

    let direction = first_segment * (1.0 / length);
    let normal = Vector3::new(-direction.y, direction.x, 0.0) * (size * CLOSED_FILLED_HALF_WIDTH);
    let base = tip - direction * size;
    let mut solid = Solid::triangle(tip, base + normal, base - normal);
    solid.common = inherit_common(common);

    Some((EntityType::Solid(solid), base))
}

/// The landing: from the connection point, along the root's direction, for
/// the landing distance. Nothing when the dogleg is disabled, has no length,
/// or has no direction.
pub(super) fn landing_line(ml: &MultiLeader, root: &LeaderRoot) -> Option<EntityType> {
    let direction_length = root.direction.length();

    if !ml.enable_dogleg || root.landing_distance <= 0.0 || direction_length <= f64::EPSILON {
        return None;
    }

    let end = root.connection_point + root.direction * (root.landing_distance / direction_length);

    Some(line_entity(
        root.connection_point,
        end,
        0.0,
        Vector3::UNIT_Z,
        &ml.common,
    ))
}

pub(super) fn emit_broken_leader_segment(
    start: Vector3,
    end: Vector3,
    breaks: &[StartEndPointPair],
    common: &EntityCommon,
    result: &mut Vec<EntityType>,
) {
    let delta = end - start;
    let length_squared = delta.length_squared();
    if length_squared <= 1e-20 {
        return;
    }
    let mut intervals: Vec<(f64, f64)> = breaks
        .iter()
        .filter_map(|pair| {
            let project =
                |point: Vector3| ((point - start).dot(&delta) / length_squared).clamp(0.0, 1.0);
            let a = project(pair.start_point);
            let b = project(pair.end_point);
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            (hi - lo > 1e-12).then_some((lo, hi))
        })
        .collect();
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cursor = 0.0;
    for (start_break, end_break) in intervals {
        if start_break > cursor + 1e-12 {
            result.push(line_entity(
                start + delta * cursor,
                start + delta * start_break,
                0.0,
                Vector3::UNIT_Z,
                common,
            ));
        }
        cursor = cursor.max(end_break);
    }
    if cursor < 1.0 - 1e-12 {
        result.push(line_entity(
            start + delta * cursor,
            end,
            0.0,
            Vector3::UNIT_Z,
            common,
        ));
    }
}

pub(super) fn leader_line_breaks(
    line: &LeaderLine,
    segment_index: usize,
) -> Vec<StartEndPointPair> {
    let mut breaks: Vec<_> = line
        .break_infos
        .iter()
        .filter(|info| info.segment_index == segment_index as i32)
        .flat_map(|info| info.break_points.iter().copied())
        .collect();
    if breaks.is_empty() && segment_index as i32 == line.segment_index {
        breaks.extend(line.break_points.iter().copied());
    }
    breaks
}

/// Explode one leader line: its arrowhead (default arrowhead only: a custom
/// block is left to the caller, which knows the document) and its segments.
pub(super) fn explode_leader_line(
    ml: &MultiLeader,
    root: &LeaderRoot,
    line: &LeaderLine,
    result: &mut Vec<EntityType>,
) {
    let mut path = leader_line_path(root, line);
    let arrowhead = if names_arrow_block(line_arrowhead_handle(ml, line)) {
        None
    } else {
        closed_filled_arrowhead(&path, line_arrowhead_size(ml, line), &ml.common)
    };

    if let Some((solid, base)) = arrowhead {
        result.push(solid);
        path[0] = base;
    }

    if line.path_type == MultiLeaderPathType::Spline && path.len() >= 3 {
        let mut spline = Spline::from_control_points((path.len().min(4) - 1) as i32, path);
        spline.common = inherit_common(&ml.common);
        result.push(EntityType::Spline(spline));
    } else {
        for (segment_index, pair) in path.windows(2).enumerate() {
            let breaks = leader_line_breaks(line, segment_index);
            emit_broken_leader_segment(pair[0], pair[1], &breaks, &ml.common, result);
        }
    }
}

pub(super) fn explode_multileader(ml: &MultiLeader) -> Vec<EntityType> {
    // Decompose MultiLeader the way ACAD's EXPLODE does: per leader line,
    // the arrowhead and the segments down to the root's connection point;
    // per root, the landing; then the text.
    let mut result = Vec::new();

    if ml.path_type != MultiLeaderPathType::Invisible {
        for root in &ml.context.leader_roots {
            for line in &root.lines {
                explode_leader_line(ml, root, line, &mut result);
            }

            result.extend(landing_line(ml, root));
        }
    }

    // Add text content if available.
    if ml.context.has_text_contents && !ml.context.text_string.is_empty() {
        let text = MText {
            common: inherit_common(&ml.common),
            value: ml.context.text_string.clone(),
            insertion_point: ml.context.text_location,
            height: ml.context.text_height,
            rectangle_width: ml.context.text_width,
            rectangle_height: None,
            rotation: ml.context.text_rotation,
            style: "Standard".to_string(),
            attachment_point: AttachmentPoint::TopLeft,
            drawing_direction: DrawingDirection::LeftToRight,
            line_spacing_factor: ml.context.line_spacing_factor,
            normal: ml.context.text_normal,
            ..MText::new()
        };
        result.push(EntityType::MText(Box::new(text)));
    }

    result
}

pub(super) fn explode_multileader_native(ml: &MultiLeader) -> Vec<EntityType> {
    let landing_points: Vec<Vector3> = ml
        .context
        .leader_roots
        .iter()
        .map(|root| root.connection_point)
        .collect();
    let mut result = Vec::new();
    for part in explode_multileader(ml) {
        match part {
            EntityType::Solid(solid) => {
                let mut polyline = LwPolyline::new();
                polyline.common = solid.common.clone();
                polyline.add_point(Vector2::new(solid.first_corner.x, solid.first_corner.y));
                polyline.add_point(Vector2::new(solid.second_corner.x, solid.second_corner.y));
                polyline.add_point(Vector2::new(solid.third_corner.x, solid.third_corner.y));
                polyline.close();
                result.push(EntityType::LwPolyline(polyline));
            }
            EntityType::Line(line)
                if landing_points
                    .iter()
                    .any(|point| line.start.distance(point) <= 1e-9) => {}
            other => result.push(other),
        }
    }
    result
}
