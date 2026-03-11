# acadrust

[![Crates.io](https://img.shields.io/crates/v/acadrust.svg)](https://crates.io/crates/acadrust)
[![Documentation](https://docs.rs/acadrust/badge.svg)](https://docs.rs/acadrust)
[![License: MPL 2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

**A pure Rust library for reading and writing CAD files (DXF and DWG).**

Inspired by [ACadSharp](https://github.com/DomCR/ACadSharp). Supports DXF (ASCII & Binary) and DWG (R13–R2018).

## Quick Start

```toml
[dependencies]
acadrust = "0.2.10"
```

```rust
use acadrust::{CadDocument, DxfReader, DxfWriter};

fn main() -> acadrust::Result<()> {
    // Read
    let doc = DxfReader::from_file("input.dxf")?.read()?;
    println!("{} entities", doc.entities().count());

    // Write
    let writer = DxfWriter::new(doc);
    writer.write_to_file("output.dxf")?;
    Ok(())
}
```

## Features

- **DXF Read/Write** — ASCII and Binary formats, R12–R2018+
- **DWG Read/Write** — Native binary, R13–R2018 (208/208 roundtrip-perfect)
- **41 Entity Types** — Lines, arcs, polylines, hatches, dimensions, 3D solids, viewports, and more
- **3D Solid Creation** — ACIS SAT/SAB builder for box, cylinder, cone, sphere, torus, wedge, pyramid
- **Paper Space & Layouts** — Multiple layouts, viewport-to-layout ownership, `add_layout()` API
- **Tables & Objects** — Layers, linetypes, styles, dictionaries, layouts, materials
- **Serde Support** — Optional `Serialize`/`Deserialize` for all types (`features = ["serde"]`)
- **Failsafe Mode** — Error-tolerant parsing with structured diagnostics
- **Encoding Support** — ~40 code pages for pre-2007 files

## File Version Support

| Version | AutoCAD | DXF | DWG |
|---------|---------|-----|-----|
| AC1009 | R12 | ✅ | — |
| AC1012–AC1014 | R13–R14 | ✅ | ✅ |
| AC1015–AC1032 | 2000–2018+ | ✅ | ✅ |

## Examples

<details>
<summary>DWG Read/Write</summary>

```rust
use acadrust::{CadDocument, DwgWriter};
use acadrust::io::dwg::DwgReader;
use acadrust::entities::*;
use acadrust::types::{Color, Vector3};

fn main() -> acadrust::Result<()> {
    // Read DWG
    let mut reader = DwgReader::from_file("drawing.dwg")?;
    let doc = reader.read()?;

    // Create & Write DWG
    let mut doc = CadDocument::new();
    let mut line = Line::from_coords(0.0, 0.0, 0.0, 100.0, 50.0, 0.0);
    line.common.color = Color::RED;
    doc.add_entity(EntityType::Line(line))?;
    DwgWriter::write_to_file("output.dwg", &doc)?;
    Ok(())
}
```
</details>

<details>
<summary>Paper Space Layouts & Viewports</summary>

```rust
use acadrust::{CadDocument, DxfVersion, DxfWriter};
use acadrust::entities::{EntityType, Viewport};
use acadrust::types::Vector3;

fn main() -> acadrust::Result<()> {
    let mut doc = CadDocument::with_version(DxfVersion::AC1027);

    // Add geometry to model space
    let line = acadrust::entities::Line {
        start: Vector3::new(0.0, 0.0, 0.0),
        end: Vector3::new(100.0, 100.0, 0.0),
        ..Default::default()
    };
    doc.add_entity(EntityType::Line(line))?;

    // Add viewport to default Layout1
    let mut vp = Viewport::new();
    vp.id = 1;
    vp.center = Vector3::new(148.5, 105.0, 0.0);
    vp.width = 297.0;
    vp.height = 210.0;
    doc.add_paper_space_entity(EntityType::Viewport(vp))?;

    // Create additional layouts
    doc.add_layout("Layout2")?;
    let mut vp2 = Viewport::with_size(Vector3::new(200.0, 150.0, 0.0), 400.0, 300.0);
    vp2.id = 1;
    doc.add_entity_to_layout(EntityType::Viewport(vp2), "Layout2")?;

    DxfWriter::new(doc).write_to_file("layouts.dxf")?;
    Ok(())
}
```
</details>

<details>
<summary>Serde / JSON</summary>

```rust
use acadrust::{CadDocument, DxfReader};

fn main() -> acadrust::Result<()> {
    let doc = DxfReader::from_file("drawing.dxf")?.read()?;
    let json = serde_json::to_string_pretty(&doc).unwrap();
    let doc2: CadDocument = serde_json::from_str(&json).unwrap();
    println!("Entities: {}", doc2.entities().count());
    Ok(())
}
```
</details>

## Documentation

Full API docs: [docs.rs/acadrust](https://docs.rs/acadrust)

---

## Changelog

### 0.2.10

- **Paper space & layout support** — `add_paper_space_entity()`, `add_entity_to_layout()`, `add_layout()` API for creating viewports in multiple paper space layouts
- **Correct DXF paper space structure** — Active layout (`*Paper_Space`) entities in ENTITIES section with code 67; non-active layouts (`*Paper_Space0`, `*Paper_Space1`, …) entities inside BLOCK definitions
- **AutoCAD AUDIT compatibility** — Fixed code 67 paper space flag, MLineStyle angle conversion (radians→degrees), AcDbPlotSettings flag, viewport owner handles
- **DXF reader** — Proper handling of code 67 (paper space flag) in common entity parsing

### 0.2.9

- **ACIS 3DSOLID write support** — SAT text builder (R2000–R2007) and SAB binary (R2013+) with primitives: box, wedge, pyramid, cylinder, cone, sphere, torus
- **`SatDocument` builder API** — `add_plane_surface`, `add_cone_surface`, `add_sphere_surface`, `add_torus_surface`, `add_straight_curve`, `add_ellipse_curve`
- **208/208 DWG roundtrip integrity** — Zero field drift across 26 entity types × 8 versions

### 0.2.8

- **DWG binary read** — Full DWG reader for R13 through R2018
- **DWG binary write** — Full DWG writer for R13 through R2018
- **Handle resolution** — Automatic owner handle assignment after read

### 0.2.7

- **Optional serde support** — `Serialize`/`Deserialize` for all document types with `features = ["serde"]`
- **JSON/YAML round-trip** — Full document serialization and deserialization

### 0.2.6

- **41 entity types** — Added MultiLeader, Table, MLine, Mesh, Underlay, Ole2Frame, Wipeout, Shape, and more
- **Objects** — Dictionaries, Groups, Layouts, MLineStyle, MultiLeaderStyle, TableStyle, PlotSettings, Scale, Materials, VisualStyle, GeoData
- **CLASSES section** — Full read/write support
- **Extended data (XData)** — Full support for application-specific extended data
- **Reactors & extension dictionaries** — Read/write for all entity and object types

### 0.2.0–0.2.5

- ASCII and Binary DXF read/write
- Core entity types (Point, Line, Circle, Arc, Ellipse, Polyline, LwPolyline, Text, MText, Spline, Dimension, Hatch, Solid, Face3D, Insert, Viewport)
- Table system (Layer, LineType, TextStyle, DimStyle, BlockRecord, AppId, View, VPort, UCS)
- Encoding support (~40 code pages)
- Failsafe reading mode
- Unknown entity preservation

---

## License

MPL-2.0 — see [LICENSE](LICENSE).

## Acknowledgments

- [ACadSharp](https://github.com/DomCR/ACadSharp) — the C# library that inspired this project

