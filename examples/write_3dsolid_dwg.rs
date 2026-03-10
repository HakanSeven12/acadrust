//! Example: write DWG files containing various 3DSOLID shapes.
//!
//! Demonstrates the SAT builder API with seven different solid shapes:
//!
//! - **Box** (10×10×10) — 6 planar faces, 12 edges, 8 vertices
//! - **Wedge** (right triangular prism) — 5 planar faces, 9 edges, 6 vertices
//! - **Pyramid** (square base, apex) — 5 planar faces, 8 edges, 5 vertices
//! - **Cylinder** (radius 5, height 10) — 2 planar caps + 1 cylindrical surface
//! - **Cone** (base radius 5, height 10) — 1 conical surface + 1 base cap
//! - **Sphere** (radius 5) — 1 spherical surface, no edges/vertices
//! - **Torus** (major 5, minor 2) — 1 toroidal surface, no edges/vertices
//!
//! Each shape is written as R2013 (AC1027) DWG. The box is also written
//! at R2000 and R2004 for multi-version testing.
//!
//! ```
//! cargo run --example write_3dsolid_dwg
//! ```

use acadrust::{CadDocument, DwgWriter, DxfVersion, EntityType};
use acadrust::entities::Solid3D;
use acadrust::entities::acis::{SatDocument, primitives};

fn main() -> acadrust::Result<()> {
    // ── 1. Build all shapes ─────────────────────────────────────────
    let box_sat = primitives::build_box([0.0, 0.0, 0.0], 10.0, 10.0, 10.0);
    let wedge_sat = primitives::build_wedge([0.0, 0.0, 0.0], 10.0, 10.0, 10.0);
    let pyramid_sat = primitives::build_pyramid([0.0, 0.0, 0.0], 10.0, 10.0);
    let cylinder_sat = primitives::build_cylinder([0.0, 0.0, 0.0], 5.0, 10.0);
    let cone_sat = primitives::build_cone([0.0, 0.0, 0.0], 5.0, 10.0);
    let sphere_sat = primitives::build_sphere([0.0, 0.0, 0.0], 5.0);
    let torus_sat = primitives::build_torus([0.0, 0.0, 0.0], 5.0, 2.0);

    // Print and validate all shapes
    print_sat_info("Box", &box_sat);
    print_sat_info("Wedge", &wedge_sat);
    print_sat_info("Pyramid", &pyramid_sat);
    print_sat_info("Cylinder", &cylinder_sat);
    print_sat_info("Cone", &cone_sat);
    print_sat_info("Sphere", &sphere_sat);
    print_sat_info("Torus", &torus_sat);

    // ── 2. Write box at multiple DWG versions ───────────────────────
    println!("\n=== Writing DWG files ===");
    write_solid("solid3d_r2000.dwg", DxfVersion::AC1015, &box_sat)?;
    write_solid("solid3d_r2004.dwg", DxfVersion::AC1018, &box_sat)?;
    write_solid("box_r2013.dwg", DxfVersion::AC1027, &box_sat)?;

    // ── 3. Write each shape at R2013 ────────────────────────────────
    write_solid("wedge_r2013.dwg", DxfVersion::AC1027, &wedge_sat)?;
    write_solid("pyramid_r2013.dwg", DxfVersion::AC1027, &pyramid_sat)?;
    write_solid("cylinder_r2013.dwg", DxfVersion::AC1027, &cylinder_sat)?;
    write_solid("cone_r2013.dwg", DxfVersion::AC1027, &cone_sat)?;
    write_solid("sphere_r2013.dwg", DxfVersion::AC1027, &sphere_sat)?;
    write_solid("torus_r2013.dwg", DxfVersion::AC1027, &torus_sat)?;

    // ── 4. Write each shape at R2007 ────────────────────────────────
    write_solid("box_r2007.dwg", DxfVersion::AC1021, &box_sat)?;
    write_solid("wedge_r2007.dwg", DxfVersion::AC1021, &wedge_sat)?;
    write_solid("pyramid_r2007.dwg", DxfVersion::AC1021, &pyramid_sat)?;
    write_solid("cylinder_r2007.dwg", DxfVersion::AC1021, &cylinder_sat)?;
    write_solid("cone_r2007.dwg", DxfVersion::AC1021, &cone_sat)?;
    write_solid("sphere_r2007.dwg", DxfVersion::AC1021, &sphere_sat)?;
    write_solid("torus_r2007.dwg", DxfVersion::AC1021, &torus_sat)?;

    // ── 5. Write cylinder at R2000 for SAT-text testing ─────────────
    write_solid("cylinder_r2000.dwg", DxfVersion::AC1015, &cylinder_sat)?;

    // ── 6. Read-back verification ───────────────────────────────────
    {
        use acadrust::DwgReader;
        println!("\n=== Read-back verification ===");

        for path in &[
            "solid3d_r2000.dwg",
            "solid3d_r2004.dwg",
            "box_r2013.dwg",
            "wedge_r2013.dwg",
            "pyramid_r2013.dwg",
            "cylinder_r2013.dwg",
            "cone_r2013.dwg",
            "sphere_r2013.dwg",
            "torus_r2013.dwg",
            "box_r2007.dwg",
            "wedge_r2007.dwg",
            "pyramid_r2007.dwg",
            "cylinder_r2007.dwg",
            "cone_r2007.dwg",
            "sphere_r2007.dwg",
            "torus_r2007.dwg",
            "cylinder_r2000.dwg",
        ] {
            let mut reader = DwgReader::from_file(path)?;
            let doc = reader.read()?;
            let solids: Vec<&Solid3D> = doc.entities().filter_map(|e| {
                if let EntityType::Solid3D(s) = e { Some(s) } else { None }
            }).collect();

            let label = path.trim_end_matches(".dwg");
            if let Some(s) = solids.first() {
                let parsed = s.parse_sat();
                let (b, f, e, v) = parsed.as_ref().map_or(
                    (0, 0, 0, 0),
                    |p| (p.bodies().len(), p.faces().len(), p.edges().len(), p.vertices().len()),
                );
                println!("  {}: {} solid(s), has_data={}, bodies={}, faces={}, edges={}, vertices={}",
                    label, solids.len(), s.acis_data.has_data(), b, f, e, v);
            } else {
                println!("  {}: no Solid3D entities found", label);
            }
        }
    }

    println!("\nDone! Open any .dwg file in AutoCAD/IntelliCAD.");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
//  Helpers
// ════════════════════════════════════════════════════════════════════════════

fn write_solid(path: &str, version: DxfVersion, sat: &SatDocument) -> acadrust::Result<()> {
    let mut solid = Solid3D::new();
    solid.set_sat_document(sat);
    solid.common.layer = "0".to_string();

    let mut doc = CadDocument::with_version(version);
    doc.add_entity(EntityType::Solid3D(solid))?;

    DwgWriter::write_to_file(path, &doc)?;
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    println!("  {} ({} bytes, {:?})", path, size, version);
    Ok(())
}

fn print_sat_info(label: &str, sat: &SatDocument) {
    let text = sat.to_sat_string();
    let errors = sat.validate();
    let bodies = sat.bodies().len();
    let faces = sat.faces().len();
    let edges = sat.edges().len();
    let vertices = sat.vertices().len();
    println!("[{}] SAT: {} bytes, {} bodies, {} faces, {} edges, {} vertices, {} warnings",
        label, text.len(), bodies, faces, edges, vertices, errors.len());
    if !errors.is_empty() {
        for e in &errors {
            println!("  WARNING: {:?}", e);
        }
    }
}
