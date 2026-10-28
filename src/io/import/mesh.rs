//! Shared triangle assembly for the import pipelines.
//!
//! Format parsers turn their files into positions and triangle indices. This
//! module welds those vertices once, drops faces that cite a missing vertex,
//! and builds column-major transforms used by the scene-graph importers.

use std::collections::HashMap;

use crate::entities::mesh::{Mesh, MeshFace};
use crate::types::Vector3;

/// Column-major identity matrix.
pub(super) const IDENTITY: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// Welds triangle corners into a [`Mesh`].
pub(super) struct TriangleAssembler {
    merge: bool,
    tolerance: f64,
    inv_tol: f64,
    tol2: f64,
    vertices: Vec<Vector3>,
    faces: Vec<MeshFace>,
    cells: HashMap<(i64, i64, i64), Vec<usize>>,
}

impl TriangleAssembler {
    pub(super) fn new(merge: bool, tolerance: f64) -> Self {
        let tolerance = if tolerance.is_finite() {
            tolerance
        } else {
            0.0
        };
        let inv_tol = if tolerance > 0.0 {
            1.0 / tolerance
        } else {
            1e9
        };
        Self {
            merge,
            tolerance,
            inv_tol,
            tol2: tolerance * tolerance,
            vertices: Vec::new(),
            faces: Vec::new(),
            cells: HashMap::new(),
        }
    }

    pub(super) fn with_triangle_capacity(merge: bool, tolerance: f64, triangles: usize) -> Self {
        let mut assembler = Self::new(merge, tolerance);
        assembler.faces.reserve(triangles);
        assembler.vertices.reserve(if merge {
            triangles
        } else {
            triangles.saturating_mul(3)
        });
        assembler
    }

    /// Adds one triangle. Non-finite corners are ignored. Welded corners that
    /// collapse to a line or a point are ignored as well.
    pub(super) fn push_triangle(&mut self, a: Vector3, b: Vector3, c: Vector3) {
        if !is_finite(a) || !is_finite(b) || !is_finite(c) {
            return;
        }
        if self.merge {
            let i0 = self.weld(a);
            let i1 = self.weld(b);
            let i2 = self.weld(c);
            if i0 != i1 && i1 != i2 && i0 != i2 {
                self.faces.push(MeshFace::triangle(i0, i1, i2));
            }
        } else {
            let base = self.vertices.len();
            self.vertices.push(a);
            self.vertices.push(b);
            self.vertices.push(c);
            self.faces
                .push(MeshFace::triangle(base, base + 1, base + 2));
        }
    }

    pub(super) fn finish(self) -> Mesh {
        let mut mesh = Mesh::new();
        mesh.vertices = self.vertices;
        mesh.faces = self.faces;
        mesh.compute_edges();
        mesh.edges
            .sort_by(|left, right| (left.start, left.end).cmp(&(right.start, right.end)));
        mesh
    }

    fn weld(&mut self, position: Vector3) -> usize {
        let key = quantize(position, self.inv_tol);
        if self.tolerance > 0.0 {
            let mut best: Option<(usize, f64)> = None;
            for neighbor in neighbor_keys(key) {
                let Some(bucket) = self.cells.get(&neighbor) else {
                    continue;
                };
                for &index in bucket {
                    let distance = (self.vertices[index] - position).length_squared();
                    if distance <= self.tol2 && best.map_or(true, |(_, best_d)| distance < best_d) {
                        best = Some((index, distance));
                    }
                }
            }
            if let Some((index, _)) = best {
                return index;
            }
        } else if let Some(index) = self.cells.get(&key).and_then(|bucket| bucket.first()) {
            return *index;
        }

        let index = self.vertices.len();
        self.vertices.push(position);
        self.cells.entry(key).or_default().push(index);
        index
    }
}

/// Builds a mesh from indexed triangles.
///
/// A triangle with an index past `positions` is dropped instead of being
/// pinned to the origin.
pub(super) fn mesh_from_indexed(
    positions: &[[f64; 3]],
    triangles: &[[usize; 3]],
    scale: f64,
    merge: bool,
    tolerance: f64,
) -> Mesh {
    let mut assembler =
        TriangleAssembler::with_triangle_capacity(merge, tolerance, triangles.len());
    for tri in triangles {
        let Some(a) = scaled_position(positions, tri[0], scale) else {
            continue;
        };
        let Some(b) = scaled_position(positions, tri[1], scale) else {
            continue;
        };
        let Some(c) = scaled_position(positions, tri[2], scale) else {
            continue;
        };
        assembler.push_triangle(a, b, c);
    }
    assembler.finish()
}

fn scaled_position(positions: &[[f64; 3]], index: usize, scale: f64) -> Option<Vector3> {
    let position = positions.get(index)?;
    let point = Vector3::new(
        position[0] * scale,
        position[1] * scale,
        position[2] * scale,
    );
    is_finite(point).then_some(point)
}

fn is_finite(point: Vector3) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.z.is_finite()
}

fn quantize(position: Vector3, inv_tol: f64) -> (i64, i64, i64) {
    (
        quantize_axis(position.x, inv_tol),
        quantize_axis(position.y, inv_tol),
        quantize_axis(position.z, inv_tol),
    )
}

fn quantize_axis(value: f64, inv_tol: f64) -> i64 {
    let scaled = (value * inv_tol).round();
    if !scaled.is_finite() {
        return 0;
    }
    if scaled >= i64::MAX as f64 {
        i64::MAX
    } else if scaled <= i64::MIN as f64 {
        i64::MIN
    } else {
        scaled as i64
    }
}

fn neighbor_keys(key: (i64, i64, i64)) -> impl Iterator<Item = (i64, i64, i64)> {
    (-1i64..=1).flat_map(move |dz| {
        (-1i64..=1).flat_map(move |dy| {
            (-1i64..=1).map(move |dx| {
                (
                    key.0.saturating_add(dx),
                    key.1.saturating_add(dy),
                    key.2.saturating_add(dz),
                )
            })
        })
    })
}

/// Multiplies two column-major 4×4 matrices. `a * b` applies `b` first.
pub(super) fn mul_matrix(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = a[row] * b[col * 4]
                + a[4 + row] * b[col * 4 + 1]
                + a[8 + row] * b[col * 4 + 2]
                + a[12 + row] * b[col * 4 + 3];
        }
    }
    out
}

pub(super) fn transform_point(matrix: &[f64; 16], x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(
        matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12],
        matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13],
        matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14],
    )
}

pub(super) fn is_identity(matrix: &[f64; 16]) -> bool {
    matrix
        .iter()
        .zip(IDENTITY.iter())
        .all(|(value, identity)| (value - identity).abs() < 1e-15)
}

pub(super) fn translation_matrix(x: f64, y: f64, z: f64) -> [f64; 16] {
    let mut matrix = IDENTITY;
    matrix[12] = x;
    matrix[13] = y;
    matrix[14] = z;
    matrix
}

pub(super) fn scale_matrix(x: f64, y: f64, z: f64) -> [f64; 16] {
    let mut matrix = IDENTITY;
    matrix[0] = x;
    matrix[5] = y;
    matrix[10] = z;
    matrix
}

/// Rotation about an arbitrary axis. `degrees` follows the right-hand rule.
pub(super) fn rotation_axis_angle(ax: f64, ay: f64, az: f64, degrees: f64) -> [f64; 16] {
    let len = (ax * ax + ay * ay + az * az).sqrt();
    if !len.is_finite() || len == 0.0 {
        return IDENTITY;
    }
    let x = ax / len;
    let y = ay / len;
    let z = az / len;
    let (sin, cos) = degrees.to_radians().sin_cos();
    let t = 1.0 - cos;
    [
        t * x * x + cos,
        t * x * y + sin * z,
        t * x * z - sin * y,
        0.0,
        t * x * y - sin * z,
        t * y * y + cos,
        t * y * z + sin * x,
        0.0,
        t * x * z + sin * y,
        t * y * z - sin * x,
        t * z * z + cos,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

/// FBX-style Euler XYZ: scale, then X, Y, Z, then translation.
pub(super) fn trs_euler_xyz(
    translation: [f64; 3],
    rotation_deg: [f64; 3],
    scale: [f64; 3],
) -> [f64; 16] {
    let scaled = scale_matrix(scale[0], scale[1], scale[2]);
    let rotated = mul_matrix(
        &rotation_axis_angle(0.0, 0.0, 1.0, rotation_deg[2]),
        &mul_matrix(
            &rotation_axis_angle(0.0, 1.0, 0.0, rotation_deg[1]),
            &mul_matrix(
                &rotation_axis_angle(1.0, 0.0, 0.0, rotation_deg[0]),
                &scaled,
            ),
        ),
    );
    mul_matrix(
        &translation_matrix(translation[0], translation[1], translation[2]),
        &rotated,
    )
}

/// glTF node matrix from translation, quaternion `(x, y, z, w)`, and scale.
pub(super) fn matrix_from_trs(
    translation: [f64; 3],
    quaternion: [f64; 4],
    scale: [f64; 3],
) -> [f64; 16] {
    let (x, y, z, w) = (quaternion[0], quaternion[1], quaternion[2], quaternion[3]);
    let x2 = x + x;
    let y2 = y + y;
    let z2 = z + z;
    let xx = x * x2;
    let xy = x * y2;
    let xz = x * z2;
    let yy = y * y2;
    let yz = y * z2;
    let zz = z * z2;
    let wx = w * x2;
    let wy = w * y2;
    let wz = w * z2;

    [
        (1.0 - (yy + zz)) * scale[0],
        (xy + wz) * scale[0],
        (xz - wy) * scale[0],
        0.0,
        (xy - wz) * scale[1],
        (1.0 - (xx + zz)) * scale[1],
        (yz + wx) * scale[1],
        0.0,
        (xz + wy) * scale[2],
        (yz - wx) * scale[2],
        (1.0 - (xx + yy)) * scale[2],
        0.0,
        translation[0],
        translation[1],
        translation[2],
        1.0,
    ]
}

pub(super) fn transform_positions(positions: &[[f64; 3]], matrix: &[f64; 16]) -> Vec<[f64; 3]> {
    positions
        .iter()
        .map(|position| {
            let point = transform_point(matrix, position[0], position[1], position[2]);
            [point.x, point.y, point.z]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welds_vertices_across_a_cell_boundary() {
        let mut assembler = TriangleAssembler::new(true, 1.0);
        assembler.push_triangle(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(3.0, 0.0, 0.0),
            Vector3::new(0.0, 3.0, 0.0),
        );
        assembler.push_triangle(
            Vector3::new(0.6, 0.0, 0.0),
            Vector3::new(3.0, 0.0, 0.0),
            Vector3::new(0.0, 3.0, 0.0),
        );
        let mesh = assembler.finish();
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.faces.len(), 2);
    }

    #[test]
    fn drops_triangles_with_missing_vertices() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let mesh = mesh_from_indexed(&positions, &[[0, 1, 9]], 1.0, false, 1e-9);
        assert!(mesh.faces.is_empty());
        assert!(mesh.vertices.is_empty());
    }

    #[test]
    fn composes_parent_then_child_translation() {
        let world = mul_matrix(
            &translation_matrix(10.0, 0.0, 0.0),
            &translation_matrix(1.0, 0.0, 0.0),
        );
        assert_eq!(
            transform_point(&world, 0.0, 0.0, 0.0),
            Vector3::new(11.0, 0.0, 0.0)
        );
    }

    #[test]
    fn rotates_90_degrees_about_z() {
        let matrix = rotation_axis_angle(0.0, 0.0, 1.0, 90.0);
        let point = transform_point(&matrix, 1.0, 0.0, 0.0);
        assert!(point.x.abs() < 1e-10);
        assert!((point.y - 1.0).abs() < 1e-10);
        assert!(point.z.abs() < 1e-10);
    }

    #[test]
    fn gltf_quaternion_matches_the_z_rotation() {
        let half = std::f64::consts::FRAC_1_SQRT_2;
        let matrix = matrix_from_trs([0.0; 3], [0.0, 0.0, half, half], [1.0, 1.0, 1.0]);
        let point = transform_point(&matrix, 1.0, 0.0, 0.0);
        assert!(point.x.abs() < 1e-10);
        assert!((point.y - 1.0).abs() < 1e-10);
    }
}
