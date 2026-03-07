use acadrust::io::dwg::{DwgReader, DwgWriter};
use acadrust::types::DxfVersion;

fn main() -> acadrust::Result<()> {
    let input = "Çizim1.dwg";

    // ── Read ──
    println!("Reading {input}...");
    let mut reader = DwgReader::from_file(input)?;
    let mut doc = reader.read()?;
    let entity_count_1 = doc.entities().count();
    let layer_count_1 = doc.layers.len();
    println!("  Version:  {:?}", doc.version);
    println!("  Entities: {entity_count_1}");
    println!("  Layers:   {layer_count_1}");

    // Try several output versions
    for &(version, label) in &[
        (DxfVersion::AC1032, "AC1032 (R2018)"),
        (DxfVersion::AC1027, "AC1027 (R2013)"),
        (DxfVersion::AC1024, "AC1024 (R2010)"),
        (DxfVersion::AC1021, "AC1021 (R2007)"),
        (DxfVersion::AC1018, "AC1018 (R2004)"),
        (DxfVersion::AC1015, "AC1015 (R2000)"),
    ] {
        let output = format!("target/Çizim1_roundtrip_{}.dwg", label.split(' ').next().unwrap());
        doc.version = version;

        println!("\n── Writing as {label} → {output} ──");
        let write_result = std::panic::catch_unwind(|| {
            DwgWriter::write_to_file(&output, &doc)
        });
        match write_result {
            Ok(Ok(_)) => {
                let file_size = std::fs::metadata(&output).unwrap().len();
                println!("  Written: {file_size} bytes");

                // Read back
                match DwgReader::from_file(&output).and_then(|mut r| r.read()) {
                    Ok(doc2) => {
                        let entity_count_2 = doc2.entities().count();
                        let layer_count_2 = doc2.layers.len();
                        println!("  Read back: {entity_count_2} entities, {layer_count_2} layers");
                        if entity_count_1 == entity_count_2 {
                            println!("  PASS");
                        } else {
                            println!("  DIFF — {entity_count_1} → {entity_count_2}");
                        }
                    }
                    Err(e) => println!("  Read-back FAILED: {e}"),
                }
            }
            Ok(Err(e)) => println!("  Write FAILED: {e}"),
            Err(_) => println!("  Write PANICKED (known size limit)"),
        }
    }

    Ok(())
}
