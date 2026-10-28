use acadrust::classes::arch::{window_size, NumberRole};
use acadrust::entities::{EntityType, ExtendedEntityData};
use acadrust::DwgReader;

fn window_numbers(path: &str) -> Vec<(i64, f64)> {
    use acadrust::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
    use acadrust::io::dwg::DwgVersion;
    use acadrust::types::DxfVersion;
    let doc = DwgReader::from_file(path).unwrap().read().unwrap();
    for entity in doc.entities() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &entity.data else {
            continue;
        };
        if data.dxf_name != "AEC_WINDOW" {
            continue;
        }
        let raw = data.raw_dwg_data.as_deref().unwrap();
        let base = DwgBitReader::new(raw.to_vec(), DwgVersion::AC24, DxfVersion::AC1032);
        let limit = raw.len() as i64 * 8 - 80;
        let mut found = Vec::new();
        let mut bit = 0i64;
        while bit < limit {
            let mut cursor = base.clone();
            cursor.set_position_in_bits(bit);
            if cursor.read_2bits() == 0 {
                let value = cursor.read_raw_double();
                if value.is_finite() && (200.0..5000.0).contains(&value) {
                    let duplicate = found.last().is_some_and(|(previous, prior): &(i64, f64)| {
                        bit - previous < 8 && (value - prior).abs() < 0.5
                    });
                    if !duplicate {
                        found.push((bit, value.round()));
                    }
                }
            }
            bit += 1;
        }
        return found;
    }
    Vec::new()
}

fn diff_windows() {
    let original = window_numbers(r"AEC SAMPLE DRAWINGS\WALL_VERTICAL_PLUS_WINDOW.dwg");
    let ours = window_numbers(r"C:\Users\Hakan\AppData\Local\Temp\height-only.dwg");
    let acad = window_numbers(r"C:\Users\Hakan\AppData\Local\Temp\height-only-acad.dwg");
    println!("bit orig ours acad");
    let mut bits: Vec<i64> = original.iter().map(|(bit, _)| *bit).collect();
    for (bit, _) in ours.iter().chain(acad.iter()) {
        if !bits.contains(bit) {
            bits.push(*bit);
        }
    }
    bits.sort();
    for bit in bits {
        let a = original
            .iter()
            .find(|(item, _)| *item == bit)
            .map(|(_, value)| *value);
        let b = ours
            .iter()
            .find(|(item, _)| *item == bit)
            .map(|(_, value)| *value);
        let c = acad
            .iter()
            .find(|(item, _)| *item == bit)
            .map(|(_, value)| *value);
        if a != b || b != c {
            println!("  @{bit} {a:?} -> {b:?} -> {c:?}");
        }
    }
}

fn variants() -> Result<(), Box<dyn std::error::Error>> {
    use acadrust::DwgWriter;
    let doc = acadrust::classes::arch::window(1200.0, 1500.0, None)?;
    DwgWriter::write_to_file(r"C:\Users\Hakan\AppData\Local\Temp\window-fixed.dwg", &doc)?;
    println!("wrote window-fixed");
    Ok(())
}

fn main() {
    if std::env::args().any(|arg| arg == "--variants") {
        if let Err(error) = variants() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--diff") {
        diff_windows();
        return;
    }
    for path in [
        r"C:\Users\Hakan\AppData\Local\Temp\width-only-acad.dwg",
        r"C:\Users\Hakan\AppData\Local\Temp\height-only-acad.dwg",
        r"target\architecture-samples\created-window.dwg",
        r"C:\Users\Hakan\AppData\Local\Temp\aec-created-window-acad.dwg",
    ] {
        let doc = DwgReader::from_file(path).unwrap().read().unwrap();
        println!("\n{path}");
        let wall = doc.entities().find_map(|entity| {
            let EntityType::Extended(entity) = entity else {
                return None;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                return None;
            };
            (data.kind == acadrust::classes::arch::ArchKind::Wall).then_some(entity.common.handle)
        });
        if let Some(wall) = wall {
            println!(
                "  anchored {:?}",
                acadrust::classes::arch::entities_anchored_to(&doc, wall)
            );
        }
        for entity in doc.entities() {
            let EntityType::Extended(entity) = entity else {
                continue;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                continue;
            };
            println!(
                "  {} {:?} preview {} profile {:?}",
                data.dxf_name,
                data.kind,
                data.preview.len(),
                data.profiles.first().map(|profile| profile.len())
            );
            if data.dxf_name == "AEC_WINDOW" {
                for (index, part) in data.preview.iter().enumerate() {
                    match part {
                        acadrust::classes::arch::PreviewRecord::Polyline(points) => {
                            println!("    preview {index} polyline {}", points.len());
                        }
                        acadrust::classes::arch::PreviewRecord::Text(text) => {
                            println!("    preview {index} text {text}");
                        }
                        acadrust::classes::arch::PreviewRecord::FillOff => {
                            println!("    preview {index} filloff");
                        }
                        acadrust::classes::arch::PreviewRecord::Other(kind) => {
                            println!("    preview {index} other {kind}");
                        }
                    }
                }
            }
            if data.dxf_name == "AEC_WINDOW" {
                println!("    size {:?}", window_size(data));
                for field in &data.fields {
                    if matches!(field.role, NumberRole::Width | NumberRole::Height) {
                        println!("    {:?} @{} {}", field.role, field.bit, field.value);
                    }
                }
                if let Some(profile) = data.profiles.first() {
                    for point in profile {
                        println!("    ({:.2}, {:.2})", point.x, point.y);
                    }
                }
            }
        }
    }
}
