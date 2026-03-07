/// Roundtrip test: Read a DWG, write it, read back, compare.
///
/// Usage: cargo run --example roundtrip_test -- <path_to_dwg>
use acadrust::*;
use std::collections::HashMap;

fn entity_type_name(e: &EntityType) -> &'static str {
    match e {
        EntityType::Point(_) => "POINT",
        EntityType::Line(_) => "LINE",
        EntityType::Circle(_) => "CIRCLE",
        EntityType::Arc(_) => "ARC",
        EntityType::Ellipse(_) => "ELLIPSE",
        EntityType::Text(_) => "TEXT",
        EntityType::MText(_) => "MTEXT",
        EntityType::Solid(_) => "SOLID",
        EntityType::Face3D(_) => "3DFACE",
        EntityType::Insert(_) => "INSERT",
        EntityType::LwPolyline(_) => "LWPOLYLINE",
        EntityType::Spline(_) => "SPLINE",
        EntityType::Hatch(_) => "HATCH",
        EntityType::Dimension(_) => "DIMENSION",
        EntityType::Viewport(_) => "VIEWPORT",
        EntityType::Leader(_) => "LEADER",
        EntityType::MultiLeader(_) => "MULTILEADER",
        EntityType::MLine(_) => "MLINE",
        EntityType::Mesh(_) => "MESH",
        EntityType::Polyline2D(_) => "POLYLINE2D",
        EntityType::Polyline3D(_) => "POLYLINE3D",
        EntityType::PolyfaceMesh(_) => "PFACE",
        EntityType::PolygonMesh(_) => "POLYMESH",
        EntityType::Ray(_) => "RAY",
        EntityType::XLine(_) => "XLINE",
        EntityType::Shape(_) => "SHAPE",
        EntityType::Tolerance(_) => "TOLERANCE",
        EntityType::RasterImage(_) => "IMAGE",
        EntityType::Wipeout(_) => "WIPEOUT",
        EntityType::Ole2Frame(_) => "OLE2FRAME",
        EntityType::AttributeDefinition(_) => "ATTDEF",
        EntityType::AttributeEntity(_) => "ATTRIB",
        EntityType::Solid3D(_) => "3DSOLID",
        EntityType::Region(_) => "REGION",
        EntityType::Body(_) => "BODY",
        EntityType::Table(_) => "TABLE",
        EntityType::Underlay(_) => "UNDERLAY",
        EntityType::Block(_) => "BLOCK",
        EntityType::BlockEnd(_) => "ENDBLK",
        EntityType::Seqend(_) => "SEQEND",
        EntityType::Polyline(_) => "POLYLINE",
        EntityType::Unknown(_) => "UNKNOWN",
    }
}

fn dump_info(label: &str, doc: &CadDocument) {
    println!("\n{}", "=".repeat(60));
    println!("[{}] Version: {:?}", label, doc.version);
    println!("[{}] Entity count: {}", label, doc.entity_count());

    // Entity type breakdown
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for e in doc.entities() {
        *counts.entry(entity_type_name(e)).or_default() += 1;
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    println!("[{}] Entity types:", label);
    for (name, count) in &sorted {
        println!("  {:<20} {}", name, count);
    }

    // Layers
    println!("[{}] Layers ({}):", label, doc.layers.len());
    for layer in doc.layers.iter() {
        println!("  {:<20} color={:?} frozen={} off={} locked={} ltype={}",
            layer.name, layer.color, layer.flags.frozen, layer.flags.off,
            layer.flags.locked, layer.line_type);
    }

    // Block records
    println!("[{}] Block records ({}):", label, doc.block_records.len());
    for br in doc.block_records.iter() {
        println!("  {:<30} handle={:?} entities={} block_entity={:?} endblk={:?}",
            br.name, br.handle, br.entities.len(),
            br.block_entity_handle, br.block_end_handle);
    }

    // Text styles
    println!("[{}] Text styles ({}):", label, doc.text_styles.len());
    for ts in doc.text_styles.iter() {
        println!("  {:<20} font={} height={}", ts.name, ts.font_file, ts.height);
    }

    // Line types
    println!("[{}] Line types ({}):", label, doc.line_types.len());
    for lt in doc.line_types.iter() {
        println!("  {:<20} elements={} pattern_len={}",
            lt.name, lt.elements.len(), lt.pattern_length);
    }

    // Layer distribution of entities
    let mut layer_counts: HashMap<String, usize> = HashMap::new();
    for e in doc.entities() {
        let layer = &e.common().layer;
        *layer_counts.entry(layer.clone()).or_default() += 1;
    }
    println!("[{}] Entities per layer:", label);
    let mut sorted_layers: Vec<_> = layer_counts.into_iter().collect();
    sorted_layers.sort_by(|a, b| b.1.cmp(&a.1));
    for (layer, count) in &sorted_layers {
        println!("  {:<20} {}", layer, count);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = if args.len() > 1 {
        &args[1]
    } else {
        "Çizim1.dwg"
    };

    println!("=== Reading original: {} ===", path);
    let original = match DwgReader::from_file(path) {
        Ok(mut r) => match r.read() {
            Ok(doc) => doc,
            Err(e) => { eprintln!("Read error: {}", e); return; }
        },
        Err(e) => { eprintln!("Open error: {}", e); return; }
    };
    dump_info("ORIGINAL", &original);

    // Write to temp file using AC1015 (R2000)
    let out_path = "roundtrip_test_output.dwg";
    println!("\n=== Writing roundtrip to: {} (AC1015) ===", out_path);
    let mut write_doc = original.clone();
    write_doc.version = DxfVersion::AC1015;
    match DwgWriter::write_to_file(out_path, &write_doc) {
        Ok(_) => println!("Write OK"),
        Err(e) => { eprintln!("Write error: {}", e); return; }
    }

    // Read back
    println!("\n=== Reading roundtrip: {} ===", out_path);
    let roundtrip = match DwgReader::from_file(out_path) {
        Ok(mut r) => match r.read() {
            Ok(doc) => doc,
            Err(e) => { eprintln!("Read-back error: {}", e); return; }
        },
        Err(e) => { eprintln!("Open-back error: {}", e); return; }
    };
    dump_info("ROUNDTRIP", &roundtrip);

    // Compare
    println!("\n{}", "=".repeat(60));
    println!("=== COMPARISON ===");
    let orig_count = original.entity_count();
    let rt_count = roundtrip.entity_count();
    println!("Entity count: {} -> {} (diff: {})", orig_count, rt_count,
        rt_count as isize - orig_count as isize);
    println!("Layers: {} -> {}", original.layers.len(), roundtrip.layers.len());
    println!("Block records: {} -> {}", original.block_records.len(), roundtrip.block_records.len());
    println!("Text styles: {} -> {}", original.text_styles.len(), roundtrip.text_styles.len());
    println!("Line types: {} -> {}", original.line_types.len(), roundtrip.line_types.len());

    // Clean up
    let _ = std::fs::remove_file(out_path);
    println!("\nDone.");
}
