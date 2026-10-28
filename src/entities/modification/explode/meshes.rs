use super::*;

pub(super) fn explode_mesh(mesh: &Mesh) -> Vec<EntityType> {
    // Decompose Mesh into Face3D entities (one per face).
    let common = &mesh.common;
    let mut result = Vec::with_capacity(mesh.faces.len());

    for face in &mesh.faces {
        let n = face.vertices.len();
        if n < 3 {
            continue;
        }
        let v = &face.vertices;
        let get =
            |idx: usize| -> Vector3 { mesh.vertices.get(idx).copied().unwrap_or(Vector3::ZERO) };

        if n == 3 {
            result.push(EntityType::Face3D(Face3D::triangle(
                get(v[0]),
                get(v[1]),
                get(v[2]),
            )));
        } else {
            // For quads and n-gons, fan-triangulate from vertex 0.
            for i in 1..n - 1 {
                let mut face3d = Face3D::triangle(get(v[0]), get(v[i]), get(v[i + 1]));
                face3d.common = inherit_common(common);
                result.push(EntityType::Face3D(face3d));
            }
            continue;
        }

        // Set common for properly constructed faces.
        if let Some(EntityType::Face3D(ref mut f)) = result.last_mut() {
            f.common = inherit_common(common);
        }
    }

    result
}

/// ACAD emits one 3DFACE for each mesh face, retaining quads instead of
/// introducing a diagonal that was not present in the source mesh.
pub(super) fn explode_mesh_native(mesh: &Mesh) -> Vec<EntityType> {
    let mut result = Vec::with_capacity(mesh.faces.len());
    for face in &mesh.faces {
        let indices = &face.vertices;
        if !(3..=4).contains(&indices.len())
            || indices.iter().any(|&index| index >= mesh.vertices.len())
        {
            continue;
        }
        let points: Vec<Vector3> = indices.iter().map(|&index| mesh.vertices[index]).collect();
        let mut output = if points.len() == 3 {
            Face3D::triangle(points[0], points[1], points[2])
        } else {
            Face3D::new(points[0], points[1], points[2], points[3])
        };
        output.common = inherit_common(&mesh.common);
        result.push(EntityType::Face3D(output));
    }
    result
}

pub(super) fn explode_polyface_mesh(mesh: &PolyfaceMesh) -> Vec<EntityType> {
    // Each face references vertices by 1-based index.
    let common = &mesh.common;
    let mut result = Vec::with_capacity(mesh.faces.len());

    let get_vertex = |idx: i16| -> Vector3 {
        let abs_idx = idx.unsigned_abs() as usize;
        if abs_idx == 0 || abs_idx > mesh.vertices.len() {
            Vector3::ZERO
        } else {
            mesh.vertices[abs_idx - 1].location
        }
    };

    for face in &mesh.faces {
        let p1 = get_vertex(face.index1);
        let p2 = get_vertex(face.index2);
        let p3 = get_vertex(face.index3);

        let mut face3d = if face.is_triangle() {
            Face3D::triangle(p1, p2, p3)
        } else {
            let p4 = get_vertex(face.index4);
            Face3D::new(p1, p2, p3, p4)
        };

        // Mark invisible edges from face index signs.
        if face.index1 < 0 {
            face3d.invisible_edges.set_first_invisible(true);
        }
        if face.index2 < 0 {
            face3d.invisible_edges.set_second_invisible(true);
        }
        if face.index3 < 0 {
            face3d.invisible_edges.set_third_invisible(true);
        }
        if face.index4 < 0 {
            face3d.invisible_edges.set_fourth_invisible(true);
        }

        face3d.common = inherit_common(common);
        result.push(EntityType::Face3D(face3d));
    }

    result
}

pub(super) fn explode_polyface_mesh_native(mesh: &PolyfaceMesh) -> Vec<EntityType> {
    let get_vertex = |index: i16| -> Option<Vector3> {
        let index = index.unsigned_abs() as usize;
        (index > 0 && index <= mesh.vertices.len()).then(|| mesh.vertices[index - 1].location)
    };
    let mut result = Vec::new();
    for face in &mesh.faces {
        let indices: Vec<i16> = face
            .vertex_indices()
            .into_iter()
            .filter(|index| *index != 0)
            .collect();
        let points: Vec<Vector3> = indices
            .iter()
            .filter_map(|&index| get_vertex(index))
            .collect();
        if points.len() != indices.len() {
            continue;
        }
        match points.as_slice() {
            [] => {}
            [point] => result.push(EntityType::Point(Point {
                common: inherit_common(&face.common),
                location: *point,
                ..Point::new()
            })),
            [start, end] => result.push(EntityType::Line(Line {
                common: inherit_common(&face.common),
                start: *start,
                end: *end,
                ..Line::new()
            })),
            [a, b, c] => {
                let mut output = Face3D::triangle(*a, *b, *c);
                output.common = inherit_common(&face.common);
                output.invisible_edges.set_first_invisible(face.index1 < 0);
                output.invisible_edges.set_second_invisible(face.index2 < 0);
                output.invisible_edges.set_third_invisible(face.index3 < 0);
                if let Some(color) = face.color {
                    output.common.color = color;
                }
                result.push(EntityType::Face3D(output));
            }
            [a, b, c, d] => {
                let mut output = Face3D::new(*a, *b, *c, *d);
                output.common = inherit_common(&face.common);
                output.invisible_edges.set_first_invisible(face.index1 < 0);
                output.invisible_edges.set_second_invisible(face.index2 < 0);
                output.invisible_edges.set_third_invisible(face.index3 < 0);
                output.invisible_edges.set_fourth_invisible(face.index4 < 0);
                if let Some(color) = face.color {
                    output.common.color = color;
                }
                result.push(EntityType::Face3D(output));
            }
            _ => {}
        }
    }
    result
}

pub(super) fn explode_polygon_mesh(mesh: &PolygonMeshEntity) -> Vec<EntityType> {
    // Decompose a polygon mesh grid (M × N) into Face3D entities.
    let common = &mesh.common;
    let m = mesh.m_vertex_count as usize;
    let n = mesh.n_vertex_count as usize;
    if m < 2 || n < 2 || mesh.vertices.len() < m * n {
        return Vec::new();
    }

    let get = |mi: usize, ni: usize| -> Vector3 { mesh.vertices[mi * n + ni].location };

    let mut result = Vec::with_capacity((m - 1) * (n - 1));
    for i in 0..m - 1 {
        for j in 0..n - 1 {
            let mut face = Face3D::new(get(i, j), get(i, j + 1), get(i + 1, j + 1), get(i + 1, j));
            face.common = inherit_common(common);
            result.push(EntityType::Face3D(face));
        }
    }

    result
}

pub(super) fn explode_polygon_mesh_native(mesh: &PolygonMeshEntity) -> Vec<EntityType> {
    let Ok(m) = usize::try_from(mesh.m_vertex_count) else {
        return Vec::new();
    };
    let Ok(n) = usize::try_from(mesh.n_vertex_count) else {
        return Vec::new();
    };
    if m < 2
        || n < 2
        || m.checked_mul(n)
            .is_none_or(|count| mesh.vertices.len() < count)
    {
        return Vec::new();
    }
    let close_m = mesh.flags.contains(PolygonMeshFlags::CLOSED_M);
    let close_n = mesh.flags.contains(PolygonMeshFlags::CLOSED_N);
    let m_cells = if close_m { m } else { m - 1 };
    let n_cells = if close_n { n } else { n - 1 };
    let get = |i: usize, j: usize| -> Vector3 { mesh.vertices[(i % m) * n + (j % n)].location };
    let mut result = Vec::with_capacity(m_cells.saturating_mul(n_cells));
    for i in 0..m_cells {
        for j in 0..n_cells {
            let mut face = Face3D::new(get(i, j), get(i, j + 1), get(i + 1, j + 1), get(i + 1, j));
            face.common = inherit_common(&mesh.common);
            result.push(EntityType::Face3D(face));
        }
    }
    result
}

// ============================================================================
// EntityType::explode
// ============================================================================
