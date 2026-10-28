//! Create Arch drawings, or explicitly edit the local sample drawings.
//!
//! ```text
//! cargo run --example architecture
//! cargo run --example architecture -- create target/architecture-created
//! cargo run --example architecture -- samples "AEC SAMPLE DRAWINGS" target/architecture-samples
//! ```

use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};

use acadrust::classes::arch::{
    continuous_wall_outline, continuous_walls, door_size, opening_size, set_door, set_opening,
    set_wall, set_window, window_size, ArchKind, NumberRole,
};
use acadrust::entities::{EntityType, ExtendedEntityData};
use acadrust::types::Vector3;
use acadrust::{DwgReader, DwgWriter};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.first().and_then(|arg| arg.to_str()) {
        None | Some("create") if args.len() <= 2 => {
            let out_dir = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("target/architecture-created"));
            std::fs::create_dir_all(&out_dir)?;
            write_without_inputs(&out_dir)?;
        }
        Some("samples") if args.len() <= 3 => {
            let samples = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("AEC SAMPLE DRAWINGS"));
            let out_dir = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("target/architecture-samples"));
            std::fs::create_dir_all(&out_dir)?;
            report(&samples)?;
            rewrite_vertical_wall(&samples, &out_dir)?;
            rewrite_door(&samples, &out_dir)?;
            rewrite_window(&samples, &out_dir)?;
            rewrite_opening(&samples, &out_dir)?;
            rewrite_continuous(&samples, &out_dir)?;
            remove_wall_and_door(&samples, &out_dir)?;
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: architecture [create [output-dir] | samples [sample-dir [output-dir]]]",
            )
            .into());
        }
    }
    Ok(())
}

fn write_without_inputs(out_dir: &Path) -> Result<()> {
    let vertical = acadrust::classes::arch::wall(
        Vector3::new(25000.0, 15000.0, 0.0),
        Vector3::new(25000.0, 21000.0, 0.0),
        300.0,
        3200.0,
        None,
    );
    let horizontal = acadrust::classes::arch::wall(
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(8000.0, 0.0, 0.0),
        240.0,
        3000.0,
        None,
    );
    let door = acadrust::classes::arch::door(900.0, 2100.0, None);
    let window = acadrust::classes::arch::window(1200.0, 1500.0, None);
    for (name, document) in [
        ("created-vertical-wall.dwg", vertical),
        ("created-horizontal-wall.dwg", horizontal),
        ("created-door.dwg", door),
        ("created-window.dwg", window),
    ] {
        save(&document?, out_dir, name, true)?;
    }
    Ok(())
}

fn report(samples: &Path) -> Result<()> {
    let mut files = dwgs(samples)?;
    files.sort();
    for path in files {
        let doc = DwgReader::from_file(&path)?.read()?;
        println!("\n{}", path.file_name().unwrap().to_string_lossy());
        for entity in doc.entities() {
            let EntityType::Extended(entity) = entity else {
                continue;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                continue;
            };
            let named: Vec<String> = data
                .fields
                .iter()
                .filter(|field| {
                    matches!(
                        field.role,
                        NumberRole::Width
                            | NumberRole::Height
                            | NumberRole::Length
                            | NumberRole::Section
                    )
                })
                .map(|field| format!("{:?} {}", field.role, field.value))
                .collect();
            println!(
                "  {} {:?} geometry {:?} sizes [{}] preview {}",
                data.dxf_name,
                data.kind,
                data.straight_wall_geometry(),
                named.join(", "),
                data.preview.len()
            );
        }
        let chains = continuous_walls(&doc);
        if chains.len() > 1 || chains.first().is_some_and(|chain| chain.handles.len() > 1) {
            for chain in chains {
                println!(
                    "  chain {} segments width {} height {} outline {}",
                    chain.handles.len(),
                    chain.width,
                    chain.height,
                    continuous_wall_outline(&chain.points, chain.width)
                        .map(|points| points.len())
                        .unwrap_or(0)
                );
            }
        }
    }
    Ok(())
}

fn rewrite_vertical_wall(samples: &Path, out_dir: &Path) -> Result<()> {
    let mut doc = open(samples, "WALL_VERTICAL.dwg")?;
    let mut wrote = false;
    for entity in doc.entities_mut() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &mut entity.data else {
            continue;
        };
        if data.kind != ArchKind::Wall {
            continue;
        }
        wrote = set_wall(
            data,
            &mut entity.common.graphic_data,
            Vector3::new(25000.0, 21000.0, 0.0),
            Vector3::new(25000.0, 15000.0, 0.0),
            300.0,
            3200.0,
        )
        .is_ok();
        break;
    }
    save(&doc, out_dir, "wall.dwg", wrote)
}

fn rewrite_door(samples: &Path, out_dir: &Path) -> Result<()> {
    let mut doc = open(samples, "WALL_VERTICAL_PLUS_DOOR.dwg")?;
    let mut wrote = false;
    for entity in doc.entities_mut() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &mut entity.data else {
            continue;
        };
        if data.kind != ArchKind::Door {
            continue;
        }
        let height = door_size(data).ok_or("unsupported door layout")?.height;
        wrote = set_door(data, &mut entity.common.graphic_data, 900.0, height).is_ok();
        break;
    }
    save(&doc, out_dir, "door.dwg", wrote)
}

fn rewrite_window(samples: &Path, out_dir: &Path) -> Result<()> {
    let mut doc = open(samples, "WALL_VERTICAL_PLUS_WINDOW.dwg")?;
    let mut wrote = false;
    for entity in doc.entities_mut() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &mut entity.data else {
            continue;
        };
        if data.kind != ArchKind::Window {
            continue;
        }
        let height = window_size(data).ok_or("unsupported window layout")?.height;
        wrote = set_window(data, &mut entity.common.graphic_data, 1200.0, height).is_ok();
        break;
    }
    save(&doc, out_dir, "window.dwg", wrote)
}

fn rewrite_opening(samples: &Path, out_dir: &Path) -> Result<()> {
    let mut doc = open(samples, "WALL_VERTICAL_PLUS_OPENING.dwg")?;
    let mut wrote = false;
    for entity in doc.entities_mut() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &mut entity.data else {
            continue;
        };
        if data.kind != ArchKind::Opening {
            continue;
        }
        let height = opening_size(data)
            .ok_or("unsupported opening layout")?
            .height;
        wrote = set_opening(data, &mut entity.common.graphic_data, 1500.0, height).is_ok();
        break;
    }
    save(&doc, out_dir, "opening.dwg", wrote)
}

fn rewrite_continuous(samples: &Path, out_dir: &Path) -> Result<()> {
    let mut doc = open(samples, "COUNTINIOUS_IRREGULAR_WALL.dwg")?;
    let chains = continuous_walls(&doc);
    let chain = chains.first().ok_or("no supported continuous wall chain")?;
    let delta = Vector3::new(1000.0, -500.0, 0.0);
    let moved: Vec<_> = chain.points.iter().map(|point| *point + delta).collect();
    let wrote = acadrust::classes::arch::set_continuous_wall(
        &mut doc,
        chain,
        &moved,
        chain.width,
        chain.height,
    )
    .is_ok();
    save(&doc, out_dir, "continuous-wall.dwg", wrote)
}

fn remove_wall_and_door(samples: &Path, out_dir: &Path) -> Result<()> {
    let mut doc = open(samples, "WALL_VERTICAL_PLUS_DOOR.dwg")?;
    let wall = doc.entities().find_map(|entity| {
        let EntityType::Extended(entity) = entity else {
            return None;
        };
        let ExtendedEntityData::Arch(data) = &entity.data else {
            return None;
        };
        (data.kind == ArchKind::Wall).then_some(entity.common.handle)
    });
    let wall = wall.ok_or("door fixture does not contain a wall")?;
    doc.remove_entity(wall);
    save(&doc, out_dir, "wall-removed.dwg", true)
}

fn open(samples: &Path, name: &str) -> Result<acadrust::CadDocument> {
    let path = samples.join(name);
    Ok(DwgReader::from_file(&path)?.read()?)
}

fn save(doc: &acadrust::CadDocument, out_dir: &Path, name: &str, wrote: bool) -> Result<()> {
    if !wrote {
        return Err(
            format!("{name}: expected class was absent or the edit was unsupported").into(),
        );
    }
    let path = out_dir.join(name);
    DwgWriter::write_to_file(&path, doc)?;
    println!("saved {}", path.display());
    Ok(())
}

fn dwgs(samples: &Path) -> Result<Vec<PathBuf>> {
    let files = std::fs::read_dir(samples)?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dwg"))
        })
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Err(format!("no DWG fixtures found in {}", samples.display()).into());
    }
    Ok(files)
}
