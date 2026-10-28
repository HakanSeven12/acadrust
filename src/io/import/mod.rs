//! Import external 3D file formats into [`CadDocument`].
//!
//! This module provides importers that convert common 3D interchange formats
//! into acadrust entities.  Tessellated formats produce [`Mesh`] entities.
//! The importers share one mesh assembler: vertices are welded on a tolerance
//! grid, and a face that cites a missing vertex is dropped. glTF and COLLADA
//! place each instance with its node transform. FBX applies each model's
//! local translation, Euler XYZ rotation, and scale, then the parent models.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use acadrust::io::import::{import_file, ImportConfig};
//!
//! let doc = import_file("model.stl", &ImportConfig::default())?;
//! println!("Imported {} entities", doc.entities().count());
//! ```
//!
//! # Supported formats
//!
//! | Extension | Module | Entity type |
//! |-----------|--------|-------------|
//! | `.stl`    | [`stl`] | [`Mesh`] |
//! | `.dae`    | [`collada`] | [`Mesh`] (one per geometry, layer-per-material) |
//! | `.obj`    | [`obj`] | [`Mesh`] (with MTL material support) |
//! | `.gltf` / `.glb` | [`gltf`] | [`Mesh`] (glTF 2.0 with PBR colors) |
//! | `.fbx`    | [`fbx`] | [`Mesh`] (binary and ASCII FBX) |

pub mod collada;
pub mod color_mapping;
pub mod fbx;
pub mod gltf;
mod mesh;
pub mod obj;
pub mod stl;

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::document::CadDocument;
use crate::error::{DxfError, Result};
use crate::types::Color;

// Re-export importers
pub use collada::ColladaImporter;
pub use fbx::FbxImporter;
pub use gltf::GltfImporter;
pub use obj::ObjImporter;
pub use stl::StlImporter;

/// Detected import format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    /// STL (Stereolithography) — ASCII or binary
    Stl,
    /// COLLADA (.dae) — XML-based 3D interchange
    Collada,
    /// Wavefront OBJ
    Obj,
    /// glTF 2.0 (.gltf JSON + .bin)
    Gltf,
    /// glTF Binary (.glb)
    Glb,
    /// Autodesk FBX (binary or ASCII)
    Fbx,
}

/// Configuration for file import.
#[derive(Debug, Clone)]
pub struct ImportConfig {
    /// Prefix prepended to generated layer names (default: `"imported"`).
    pub layer_prefix: String,
    /// Default color for entities when no material color is available.
    pub default_color: Color,
    /// Merge duplicate vertices within `merge_tolerance` distance (default: `true`).
    pub merge_vertices: bool,
    /// Distance tolerance for vertex merging (default: `1e-9`).
    pub merge_tolerance: f64,
    /// Uniform scale factor applied to all coordinates (default: `1.0`).
    pub scale_factor: f64,
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            layer_prefix: "imported".to_string(),
            default_color: Color::ByLayer,
            merge_vertices: true,
            merge_tolerance: 1e-9,
            scale_factor: 1.0,
        }
    }
}

/// Detect the import format from a file path's extension.
pub fn detect_format(path: &Path) -> Result<ImportFormat> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "stl" => Ok(ImportFormat::Stl),
        "dae" => Ok(ImportFormat::Collada),
        "obj" => Ok(ImportFormat::Obj),
        "gltf" => Ok(ImportFormat::Gltf),
        "glb" => Ok(ImportFormat::Glb),
        "fbx" => Ok(ImportFormat::Fbx),
        _ => Err(DxfError::ImportError(format!(
            "Unsupported import format: .{}",
            ext
        ))),
    }
}

/// Import a file into a [`CadDocument`], auto-detecting the format from the
/// file extension.
///
/// This is a convenience wrapper around the format-specific importers.
pub fn import_file(path: impl AsRef<Path>, config: &ImportConfig) -> Result<CadDocument> {
    let path = path.as_ref();
    let format = detect_format(path)?;

    match format {
        ImportFormat::Stl => StlImporter::from_file(path)?
            .with_config(config.clone())
            .import(),
        ImportFormat::Collada => ColladaImporter::from_file(path)?
            .with_config(config.clone())
            .import(),
        ImportFormat::Obj => ObjImporter::from_file(path)?
            .with_config(config.clone())
            .import(),
        ImportFormat::Gltf | ImportFormat::Glb => GltfImporter::from_file(path)?
            .with_config(config.clone())
            .import(),
        ImportFormat::Fbx => FbxImporter::from_file(path)?
            .with_config(config.clone())
            .import(),
    }
}

pub(super) fn read_path_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut file = std::fs::File::open(path).map_err(|error| {
        DxfError::ImportError(format!("Cannot open '{}': {}", path.display(), error))
    })?;
    read_reader_bytes(&mut file, &format!("'{}'", path.display()))
}

pub(super) fn read_reader_bytes<R: Read>(mut reader: R, label: &str) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    reader
        .read_to_end(&mut data)
        .map_err(|error| DxfError::ImportError(format!("Cannot read {label}: {error}")))?;
    Ok(data)
}

/// Joins `reference` onto `base_dir` when the reference stays inside that directory.
pub(super) fn resolve_relative(base_dir: &Path, reference: &str) -> Result<PathBuf> {
    if reference.is_empty() || reference.contains('\0') {
        return Err(DxfError::ImportError(
            "Import reference is empty".to_string(),
        ));
    }
    let relative = Path::new(reference);
    let escapes = relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    if escapes {
        return Err(DxfError::ImportError(format!(
            "Import reference '{reference}' escapes the source directory"
        )));
    }
    Ok(base_dir.join(relative))
}

/// Decodes a `%HH` URI path. `+` is left unchanged.
pub(super) fn percent_decode(input: &str) -> Result<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(DxfError::ImportError(format!(
                    "Invalid percent-encoding in '{input}'"
                )));
            }
            let high = from_hex(bytes[index + 1]).ok_or_else(|| {
                DxfError::ImportError(format!("Invalid percent-encoding in '{input}'"))
            })?;
            let low = from_hex(bytes[index + 2]).ok_or_else(|| {
                DxfError::ImportError(format!("Invalid percent-encoding in '{input}'"))
            })?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .map_err(|_| DxfError::ImportError(format!("Import reference '{input}' is not UTF-8")))
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_directory_references() {
        let error = resolve_relative(Path::new("models"), "../secret.mtl").unwrap_err();
        assert!(error.to_string().contains("escapes"));
    }

    #[test]
    fn joins_a_sibling_reference() {
        let path = resolve_relative(Path::new("models"), "wood.mtl").unwrap();
        assert_eq!(path, Path::new("models").join("wood.mtl"));
    }

    #[test]
    fn percent_decodes_a_space() {
        assert_eq!(
            percent_decode("buffer%20name.bin").unwrap(),
            "buffer name.bin"
        );
    }
}
