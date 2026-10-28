//! COLLADA (.dae) importer.
//!
//! Produces one [`Mesh`] entity per geometry instance, with a DXF layer per
//! unique material and diffuse-colour mapping. Nested nodes keep their
//! parent's transform. `<translate>`, `<rotate>`, and `<scale>` are applied
//! in document order before `<matrix>` elements that follow them.
//!
//! # Example
//!
//! ```rust,ignore
//! use acadrust::io::import::collada::ColladaImporter;
//! use acadrust::io::import::ImportConfig;
//!
//! let doc = ColladaImporter::from_file("scene.dae")?
//!     .with_config(ImportConfig::default())
//!     .import()?;
//! ```

pub mod parser;

use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::path::Path;

use indexmap::IndexMap;

use crate::document::CadDocument;
use crate::entities::mesh::Mesh;
use crate::entities::EntityType;
use crate::error::Result;

use super::color_mapping::{color_from_unit_rgb, create_material_layer};
use super::mesh::{is_identity, mesh_from_indexed, transform_positions};
use super::{read_path_bytes, read_reader_bytes, ImportConfig};

use parser::{parse_collada, ColladaGeometry, ColladaNode, ColladaScene};

/// COLLADA file importer.
pub struct ColladaImporter {
    data: Vec<u8>,
    config: ImportConfig,
}

impl ColladaImporter {
    /// Create an importer from a file path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            data: read_path_bytes(path.as_ref())?,
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
            data: read_reader_bytes(reader, "COLLADA data")?,
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
        let scene = self.parse()?;
        self.build_document(&scene)
    }

    /// Parse the COLLADA XML into a [`ColladaScene`].
    pub fn parse(&self) -> Result<ColladaScene> {
        let reader = BufReader::new(self.data.as_slice());
        parse_collada(reader)
    }

    /// Convert a [`ColladaScene`] into a [`CadDocument`].
    fn build_document(&self, scene: &ColladaScene) -> Result<CadDocument> {
        let mut doc = CadDocument::new();
        let scale = self.config.scale_factor;

        // Index geometries by id for lookup
        let geom_map: HashMap<&str, &ColladaGeometry> = scene
            .geometries
            .iter()
            .map(|g| (g.id.as_str(), g))
            .collect();

        for node in &scene.nodes {
            let geom_id = node
                .geometry_url
                .strip_prefix('#')
                .unwrap_or(&node.geometry_url);
            let geom = match geom_map.get(geom_id) {
                Some(g) => g,
                None => continue, // Referenced geometry not found — skip
            };

            // Build meshes grouped by material
            let meshes_by_material = build_meshes_for_geometry(
                geom,
                node,
                &scene.materials,
                scale,
                self.config.merge_vertices,
                self.config.merge_tolerance,
            );

            for (material_name, color, mut mesh) in meshes_by_material {
                if mesh.faces.is_empty() {
                    continue;
                }

                let layer_color = color_from_unit_rgb(color[0], color[1], color[2]);
                let layer_name = if material_name.is_empty() {
                    create_material_layer(
                        &mut doc,
                        &self.config.layer_prefix,
                        if node.name.is_empty() {
                            &geom.name
                        } else {
                            &node.name
                        },
                        self.config.default_color,
                    )
                } else {
                    create_material_layer(
                        &mut doc,
                        &self.config.layer_prefix,
                        &material_name,
                        layer_color,
                    )
                };

                mesh.common.layer = layer_name;
                if !material_name.is_empty() {
                    mesh.common.color = layer_color;
                }

                doc.add_entity(EntityType::Mesh(mesh))?;
            }
        }

        Ok(doc)
    }
}

/// Build Mesh entities from a geometry, grouped by material.
///
/// Returns a list of `(material_name, diffuse_color, Mesh)`.
fn build_meshes_for_geometry(
    geom: &ColladaGeometry,
    node: &ColladaNode,
    materials: &HashMap<String, parser::ColladaMaterial>,
    scale: f64,
    merge: bool,
    tolerance: f64,
) -> Vec<(String, [f32; 4], Mesh)> {
    let has_transform = !is_identity(&node.transform);

    // Group primitives by resolved material id, in the order materials first appear.
    let mut groups: IndexMap<String, Vec<usize>> = IndexMap::new();

    for prim in &geom.triangles {
        // Resolve material: symbol → binding → material id
        let mat_id = node
            .material_bindings
            .get(&prim.material_symbol)
            .cloned()
            .unwrap_or_default();

        let entry = groups.entry(mat_id).or_default();
        entry.extend_from_slice(&prim.indices);
    }

    let mut results = Vec::new();

    for (mat_id, vertex_indices) in groups {
        let diffuse = materials
            .get(&mat_id)
            .map(|m| m.diffuse)
            .unwrap_or([0.8, 0.8, 0.8, 1.0]);
        let mat_name = materials
            .get(&mat_id)
            .map(|m| m.name.clone())
            .unwrap_or_default();

        let mesh = build_mesh_from_indices(
            &geom.vertices,
            &vertex_indices,
            &node.transform,
            has_transform,
            scale,
            merge,
            tolerance,
        );

        results.push((mat_name, diffuse, mesh));
    }

    results
}

fn build_mesh_from_indices(
    positions: &[[f64; 3]],
    vertex_indices: &[usize],
    transform: &[f64; 16],
    has_transform: bool,
    scale: f64,
    merge: bool,
    tolerance: f64,
) -> Mesh {
    let triangles: Vec<[usize; 3]> = vertex_indices
        .chunks_exact(3)
        .map(|tri| [tri[0], tri[1], tri[2]])
        .collect();
    let transformed;
    let source: &[[f64; 3]] = if has_transform {
        transformed = transform_positions(positions, transform);
        &transformed
    } else {
        positions
    };
    mesh_from_indexed(source, &triangles, scale, merge, tolerance)
}

#[cfg(test)]
mod tests {
    use super::super::mesh::transform_point;
    use super::*;

    #[test]
    fn test_apply_transform_identity() {
        let id = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let point = transform_point(&id, 1.0, 2.0, 3.0);
        assert!((point.x - 1.0).abs() < 1e-10);
        assert!((point.y - 2.0).abs() < 1e-10);
        assert!((point.z - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_apply_transform_translation() {
        // Translation by (10, 20, 30) in column-major
        let m = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 10.0, 20.0, 30.0, 1.0,
        ];
        let point = transform_point(&m, 1.0, 2.0, 3.0);
        assert!((point.x - 11.0).abs() < 1e-10);
        assert!((point.y - 22.0).abs() < 1e-10);
        assert!((point.z - 33.0).abs() < 1e-10);
    }

    #[test]
    fn test_import_collada_bytes() {
        let dae = r##"<?xml version="1.0" encoding="utf-8"?>
<COLLADA xmlns="http://www.collada.org/2005/11/COLLADASchema" version="1.4.1">
  <library_effects>
    <effect id="eff0">
      <profile_COMMON><technique sid="t"><phong>
        <diffuse><color>0.0 0.0 1.0 1.0</color></diffuse>
      </phong></technique></profile_COMMON>
    </effect>
  </library_effects>
  <library_materials>
    <material id="mat0" name="Blue">
      <instance_effect url="#eff0"/>
    </material>
  </library_materials>
  <library_geometries>
    <geometry id="g0" name="Tri">
      <mesh>
        <source id="p0">
          <float_array id="p0a" count="9">0 0 0 1 0 0 0 1 0</float_array>
          <technique_common>
            <accessor source="#p0a" count="3" stride="3">
              <param name="X" type="float"/>
              <param name="Y" type="float"/>
              <param name="Z" type="float"/>
            </accessor>
          </technique_common>
        </source>
        <vertices id="v0">
          <input semantic="POSITION" source="#p0"/>
        </vertices>
        <triangles count="1" material="ms">
          <input semantic="VERTEX" source="#v0" offset="0"/>
          <p>0 1 2</p>
        </triangles>
      </mesh>
    </geometry>
  </library_geometries>
  <library_visual_scenes>
    <visual_scene id="Scene">
      <node name="N">
        <instance_geometry url="#g0">
          <bind_material><technique_common>
            <instance_material symbol="ms" target="#mat0"/>
          </technique_common></bind_material>
        </instance_geometry>
      </node>
    </visual_scene>
  </library_visual_scenes>
</COLLADA>"##;

        let importer = ColladaImporter::from_bytes(dae.as_bytes().to_vec());
        let doc = importer.import().unwrap();
        let entities: Vec<_> = doc.entities().collect();
        assert_eq!(entities.len(), 1);
        if let EntityType::Mesh(mesh) = &entities[0] {
            assert_eq!(mesh.vertices.len(), 3);
            assert_eq!(mesh.faces.len(), 1);
            // Should be on a "Blue" layer
            assert!(mesh.common.layer.contains("Blue"));
        } else {
            panic!("Expected Mesh entity");
        }
    }
}
