use super::super::transform;
use super::*;

pub(crate) fn mirror_face3d(e: &mut Face3D, transform: &Transform) {
    if !super::valid_transform(transform) {
        return;
    }
    transform::transform_face3d(e, transform);
    if transform::is_reflecting(transform) {
        std::mem::swap(&mut e.first_corner, &mut e.fourth_corner);
        std::mem::swap(&mut e.second_corner, &mut e.third_corner);
        let bits = e.invisible_edges.bits();
        e.invisible_edges =
            InvisibleEdgeFlags::from_bits((bits & !5) | ((bits & 1) << 2) | ((bits & 4) >> 2));
    }
    e.common.raw_record = None;
}

pub(crate) fn mirror_solid(e: &mut Solid, transform: &Transform) {
    if !super::valid_transform(transform) {
        return;
    }
    transform::transform_solid(e, transform);
    e.common.raw_record = None;
}

pub(crate) fn mirror_mesh(e: &mut Mesh, transform: &Transform) {
    if !super::valid_transform(transform) {
        return;
    }
    transform::transform_mesh(e, transform);
    e.common.raw_record = None;
}

pub(crate) fn mirror_polyface_mesh(e: &mut PolyfaceMesh, transform: &Transform) {
    if !super::valid_transform(transform) {
        return;
    }
    transform::transform_polyface_mesh(e, transform);
    if transform::is_reflecting(transform) {
        for face in &mut e.faces {
            face.reverse();
            face.common.raw_record = None;
        }
    }
    for vertex in &mut e.vertices {
        vertex.common.raw_record = None;
    }
    e.common.raw_record = None;
}

pub(crate) fn mirror_polygon_mesh(e: &mut PolygonMeshEntity, transform: &Transform) {
    if !super::valid_transform(transform) {
        return;
    }
    let reflecting = transform::is_reflecting(transform);
    transform::transform_polygon_mesh(e, transform);
    // POLYMESH stores its grid in M-major rows. ACAD reverses each M row
    // under a reflection so the smoothing surface and closure seams retain
    // their geometric edge ownership.
    if reflecting && e.m_vertex_count > 0 {
        let m = e.m_vertex_count as usize;
        for row in e.vertices.chunks_exact_mut(m) {
            row.reverse();
        }
    }
    e.common.raw_record = None;
}

pub(crate) fn mirror_solid3d(e: &mut Solid3D, transform: &Transform) {
    if !super::valid_transform(transform) {
        return;
    }
    transform::transform_solid3d(e, transform);
    e.common.raw_record = None;
}

pub(crate) fn mirror_region(e: &mut Region, transform: &Transform) {
    if !super::valid_transform(transform) {
        return;
    }
    transform::transform_region(e, transform);
    e.common.raw_record = None;
}

pub(crate) fn mirror_body(e: &mut Body, transform: &Transform) {
    if !super::valid_transform(transform) {
        return;
    }
    transform::transform_body(e, transform);
    e.common.raw_record = None;
}

pub(crate) fn mirror_surface(e: &mut Surface, transform: &Transform) {
    if !super::valid_transform(transform) {
        return;
    }
    transform::transform_surface(e, transform);
    e.common.raw_record = None;
}
