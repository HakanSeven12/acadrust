//! STL (Stereolithography) importer.
//!
//! Supports both ASCII and binary STL files.  Produces [`Mesh`] entities.
//!
//! # Example
//!
//! ```rust,ignore
//! use acadrust::io::import::stl::StlImporter;
//! use acadrust::io::import::ImportConfig;
//!
//! let doc = StlImporter::from_file("model.stl")?
//!     .with_config(ImportConfig::default())
//!     .import()?;
//! ```

pub mod ascii;
pub mod binary;

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

use crate::document::CadDocument;
use crate::entities::mesh::Mesh;
use crate::entities::EntityType;
use crate::error::Result;
use crate::types::Vector3;

use super::color_mapping::{create_material_layer, rgb_to_color};
use super::mesh::TriangleAssembler;
use super::{read_path_bytes, read_reader_bytes, ImportConfig};

/// An individual STL triangle.
#[derive(Debug, Clone)]
pub struct StlTriangle {
    /// Face normal (may be zero if not specified).
    pub normal: [f64; 3],
    /// Three vertices of the triangle.
    pub vertices: [[f64; 3]; 3],
    /// Optional per-facet color from binary STL attribute field (R, G, B).
    pub color: Option<(u8, u8, u8)>,
}

/// Parsed STL mesh data (format-agnostic).
#[derive(Debug, Clone)]
pub struct StlMesh {
    /// Solid name from the STL file.
    pub name: String,
    /// All triangles.
    pub triangles: Vec<StlTriangle>,
}

/// STL file importer.
///
/// Detects ASCII vs binary format automatically and converts the mesh into
/// a [`CadDocument`] containing a single [`Mesh`] entity.
pub struct StlImporter {
    data: Vec<u8>,
    config: ImportConfig,
}

impl StlImporter {
    /// Create an importer from a file path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Ok(Self {
            data: read_path_bytes(path)?,
            config: ImportConfig::default(),
        })
    }

    /// Create an importer from an in-memory buffer.
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self {
            data,
            config: ImportConfig::default(),
        }
    }

    /// Create an importer from a reader.
    pub fn from_reader<R: Read>(reader: R) -> Result<Self> {
        Ok(Self {
            data: read_reader_bytes(reader, "STL data")?,
            config: ImportConfig::default(),
        })
    }

    /// Set the import configuration.
    pub fn with_config(mut self, config: ImportConfig) -> Self {
        self.config = config;
        self
    }

    /// Perform the import and return a [`CadDocument`].
    pub fn import(&self) -> Result<CadDocument> {
        let stl_mesh = self.parse()?;
        self.build_document(&stl_mesh)
    }

    /// Parse the STL data into an [`StlMesh`].
    pub fn parse(&self) -> Result<StlMesh> {
        if is_ascii_stl(&self.data) {
            ascii::parse_ascii_stl(Cursor::new(&self.data))
        } else {
            let mut cursor = Cursor::new(&self.data);
            binary::parse_binary_stl(&mut cursor)
        }
    }

    /// Convert an [`StlMesh`] into a [`CadDocument`] with [`Mesh`] entities.
    fn build_document(&self, stl_mesh: &StlMesh) -> Result<CadDocument> {
        let mut doc = CadDocument::new();

        if stl_mesh.triangles.is_empty() {
            return Ok(doc);
        }

        let scale = self.config.scale_factor;

        // Check if any triangles have colour information
        let has_colors = stl_mesh.triangles.iter().any(|t| t.color.is_some());

        if has_colors {
            // Group triangles by colour → separate Mesh per colour on its own layer
            let grouped = group_by_color(&stl_mesh.triangles);
            for ((r, g, b), tri_indices) in &grouped {
                let color = rgb_to_color(*r, *g, *b, true);
                let layer = create_material_layer(
                    &mut doc,
                    &self.config.layer_prefix,
                    &format!("{}_{}_{}_{}", stl_mesh.name, r, g, b),
                    color,
                );
                let mesh = build_mesh(
                    stl_mesh,
                    tri_indices,
                    scale,
                    self.config.merge_vertices,
                    self.config.merge_tolerance,
                );
                if mesh.faces.is_empty() {
                    continue;
                }
                let mut mesh_entity = mesh;
                mesh_entity.common.layer = layer;
                mesh_entity.common.color = color;
                doc.add_entity(EntityType::Mesh(mesh_entity))?;
            }
        } else {
            // Single mesh, single layer
            let layer = create_material_layer(
                &mut doc,
                &self.config.layer_prefix,
                &stl_mesh.name,
                self.config.default_color,
            );
            let all_indices: Vec<usize> = (0..stl_mesh.triangles.len()).collect();
            let mut mesh = build_mesh(
                stl_mesh,
                &all_indices,
                scale,
                self.config.merge_vertices,
                self.config.merge_tolerance,
            );
            if mesh.faces.is_empty() {
                return Ok(doc);
            }
            mesh.common.layer = layer;
            doc.add_entity(EntityType::Mesh(mesh))?;
        }

        Ok(doc)
    }
}

/// Determine whether a byte buffer looks like an ASCII STL.
///
/// A binary file whose 80-byte header happens to start with `solid` is still
/// binary when its length is `84 + 50 * triangle_count`, including a length
/// that has a few trailing bytes and no `facet` keyword.
fn is_ascii_stl(data: &[u8]) -> bool {
    let starts_solid = data.len() >= 5
        && data[..5].eq_ignore_ascii_case(b"solid")
        && (data.len() == 5 || data[5].is_ascii_whitespace());

    if data.len() >= 84 {
        let count = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as u64;
        let expected = 84u64.saturating_add(count.saturating_mul(50));
        if data.len() as u64 == expected {
            return false;
        }
    }

    if !starts_solid {
        return false;
    }
    if data.len() < 84 {
        return true;
    }

    let sample = &data[..data.len().min(65_536)];
    sample
        .windows(5)
        .any(|window| window.eq_ignore_ascii_case(b"facet"))
}

/// Group triangle indices by their colour.
fn group_by_color(triangles: &[StlTriangle]) -> Vec<((u8, u8, u8), Vec<usize>)> {
    let mut map: HashMap<(u8, u8, u8), Vec<usize>> = HashMap::new();
    for (i, tri) in triangles.iter().enumerate() {
        let key = tri.color.unwrap_or((255, 255, 255));
        map.entry(key).or_default().push(i);
    }
    // Sort by key for deterministic output
    let mut entries: Vec<_> = map.into_iter().collect();
    entries.sort_by_key(|&(k, _)| k);
    entries
}

/// Build a [`Mesh`] entity from a subset of triangles in an [`StlMesh`].
fn build_mesh(
    stl_mesh: &StlMesh,
    tri_indices: &[usize],
    scale: f64,
    merge: bool,
    tolerance: f64,
) -> Mesh {
    let mut assembler =
        TriangleAssembler::with_triangle_capacity(merge, tolerance, tri_indices.len());
    for &ti in tri_indices {
        let tri = &stl_mesh.triangles[ti];
        assembler.push_triangle(
            Vector3::new(
                tri.vertices[0][0] * scale,
                tri.vertices[0][1] * scale,
                tri.vertices[0][2] * scale,
            ),
            Vector3::new(
                tri.vertices[1][0] * scale,
                tri.vertices[1][1] * scale,
                tri.vertices[1][2] * scale,
            ),
            Vector3::new(
                tri.vertices[2][0] * scale,
                tri.vertices[2][1] * scale,
                tri.vertices[2][2] * scale,
            ),
        );
    }
    assembler.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ascii_stl() {
        assert!(is_ascii_stl(b"solid test\n"));
        assert!(is_ascii_stl(b"solid \n"));
        assert!(!is_ascii_stl(b"not_solid\n"));
    }

    #[test]
    fn binary_header_that_starts_with_solid_stays_binary() {
        let mut data = vec![0u8; 84 + 50];
        data[..6].copy_from_slice(b"solid ");
        data[80..84].copy_from_slice(&1u32.to_le_bytes());
        assert!(!is_ascii_stl(&data));
        data.push(0);
        assert!(!is_ascii_stl(&data));

        let mut ascii = b"solid name\n".to_vec();
        ascii.resize(100, b' ');
        ascii.extend_from_slice(b"facet normal 0 0 1\n");
        assert!(is_ascii_stl(&ascii));
    }

    #[test]
    fn test_roundtrip_ascii() {
        let stl = b"solid cube\n\
            facet normal 0 0 1\n\
            outer loop\n\
            vertex 0 0 0\n\
            vertex 1 0 0\n\
            vertex 0 1 0\n\
            endloop\n\
            endfacet\n\
            endsolid cube\n";

        let importer = StlImporter::from_bytes(stl.to_vec());
        let doc = importer.import().unwrap();
        let entities: Vec<_> = doc.entities().collect();
        assert_eq!(entities.len(), 1);
    }

    #[test]
    fn test_scale_factor() {
        let stl = b"solid s\n\
            facet normal 0 0 1\n\
            outer loop\n\
            vertex 1 2 3\n\
            vertex 4 5 6\n\
            vertex 7 8 9\n\
            endloop\n\
            endfacet\n\
            endsolid s\n";

        let mut config = ImportConfig::default();
        config.scale_factor = 10.0;
        config.merge_vertices = false;

        let importer = StlImporter::from_bytes(stl.to_vec()).with_config(config);
        let doc = importer.import().unwrap();
        let entities: Vec<_> = doc.entities().collect();
        assert_eq!(entities.len(), 1);
        if let EntityType::Mesh(mesh) = &entities[0] {
            assert_eq!(mesh.vertices[0], Vector3::new(10.0, 20.0, 30.0));
        } else {
            panic!("Expected Mesh entity");
        }
    }

    #[test]
    fn test_vertex_merging() {
        // Two triangles sharing an edge (vertices at 1,0,0 and 0,1,0)
        let stl = b"solid m\n\
            facet normal 0 0 1\n\
            outer loop\n\
            vertex 0 0 0\n\
            vertex 1 0 0\n\
            vertex 0 1 0\n\
            endloop\n\
            endfacet\n\
            facet normal 0 0 1\n\
            outer loop\n\
            vertex 1 0 0\n\
            vertex 1 1 0\n\
            vertex 0 1 0\n\
            endloop\n\
            endfacet\n\
            endsolid m\n";

        let mut config = ImportConfig::default();
        config.merge_vertices = true;

        let importer = StlImporter::from_bytes(stl.to_vec()).with_config(config);
        let stl_mesh = importer.parse().unwrap();
        assert_eq!(stl_mesh.triangles.len(), 2);

        let doc = importer.import().unwrap();
        let entities: Vec<_> = doc.entities().collect();
        if let EntityType::Mesh(mesh) = &entities[0] {
            // With merging: 4 unique vertices instead of 6
            assert_eq!(mesh.vertices.len(), 4);
            assert_eq!(mesh.faces.len(), 2);
        } else {
            panic!("Expected Mesh entity");
        }
    }
}
