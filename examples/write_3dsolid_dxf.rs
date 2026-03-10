//! Example: write DXF files containing 3DSOLID entities with valid SAT data.
//!
//! Builds all seven ACIS primitive shapes using the SAT builder API,
//! wraps each in a `Solid3D` entity, and writes R2013 DXF files.
//!
//! Primitives:
//! - **Box** (10×10×10) — 6 faces, 12 edges, 8 vertices
//! - **Wedge** (right triangular prism) — 5 faces, 9 edges, 6 vertices
//! - **Pyramid** (square base, apex) — 5 faces, 8 edges, 5 vertices
//! - **Cylinder** (radius 5, height 10) — 3 faces, 3 edges, 2 vertices
//! - **Cone** (base radius 5, height 10) — 2 faces, 1 edge, 1 vertex
//! - **Sphere** (radius 5) — 1 face, 0 edges, 0 vertices
//! - **Torus** (major 5, minor 2) — 1 face, 0 edges, 0 vertices
//!
//! ```
//! cargo run --example write_3dsolid_dxf
//! ```

use acadrust::{CadDocument, DxfWriter, DxfVersion, EntityType};
use acadrust::entities::Solid3D;
use acadrust::entities::acis::primitives;

fn main() -> acadrust::Result<()> {
    let shapes: Vec<(&str, _)> = vec![
        ("box",      primitives::build_box([0.0, 0.0, 0.0], 10.0, 10.0, 10.0)),
        ("wedge",    primitives::build_wedge([0.0, 0.0, 0.0], 10.0, 10.0, 10.0)),
        ("pyramid",  primitives::build_pyramid([0.0, 0.0, 0.0], 10.0, 10.0)),
        ("cylinder", primitives::build_cylinder([0.0, 0.0, 0.0], 5.0, 10.0)),
        ("cone",     primitives::build_cone([0.0, 0.0, 0.0], 5.0, 10.0)),
        ("sphere",   primitives::build_sphere([0.0, 0.0, 0.0], 5.0)),
        ("torus",    primitives::build_torus([0.0, 0.0, 0.0], 5.0, 2.0)),
    ];

    println!("=== Building SAT primitives ===");
    for (name, sat) in &shapes {
        let errors = sat.validate();
        println!("  {:10} {} bodies, {} faces, {} edges, {} vertices, {} warnings",
            name,
            sat.bodies().len(),
            sat.faces().len(),
            sat.edges().len(),
            sat.vertices().len(),
            errors.len(),
        );
        for e in &errors {
            println!("    WARNING: {:?}", e);
        }
    }

    let versions = vec![
        ("r2013", DxfVersion::AC1027),
        ("r2007", DxfVersion::AC1021),
    ];

    for (ver_label, ver) in &versions {
        println!("\n=== Writing DXF files (version: {:?}) ===", ver);
        for (name, sat) in &shapes {
            let path = format!("{}_{}.dxf", name, ver_label);

            let mut solid = Solid3D::new();
            solid.set_sat_document(sat);
            solid.common.layer = "0".to_string();

            let mut doc = CadDocument::with_version(*ver);
            doc.add_entity(EntityType::Solid3D(solid))?;

            let writer = DxfWriter::new(doc);
            writer.write_to_file(&path)?;

            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            println!("  {} ({} bytes)", path, size);
        }
    }

    println!("\nDone! Open any .dxf file in AutoCAD/IntelliCAD.");
    Ok(())
}