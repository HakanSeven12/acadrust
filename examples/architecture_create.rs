//! Create native Arch drawings without sample files.
//!
//! Run `cargo run --example architecture_create -- target/architecture-created`.

use acadrust::classes::arch::{door, wall, window};
use acadrust::types::Vector3;
use acadrust::DwgWriter;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/architecture-created"));
    std::fs::create_dir_all(&output)?;
    let documents = [
        (
            "horizontal-wall.dwg",
            wall(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(8000.0, 0.0, 0.0),
                240.0,
                3000.0,
                None,
            )?,
        ),
        (
            "vertical-wall.dwg",
            wall(
                Vector3::new(25000.0, 15000.0, 0.0),
                Vector3::new(25000.0, 21000.0, 0.0),
                300.0,
                3200.0,
                None,
            )?,
        ),
        (
            "diagonal-wall.dwg",
            wall(
                Vector3::new(-120.0, 50.0, 0.0),
                Vector3::new(2880.0, 4050.0, 0.0),
                175.0,
                2800.0,
                None,
            )?,
        ),
        ("door.dwg", door(2600.0, 2100.0, None)?),
        ("window.dwg", window(1800.0, 2400.0, None)?),
    ];
    for (name, document) in documents {
        let path = output.join(name);
        DwgWriter::write_to_file(&path, &document)?;
        println!("{}", path.display());
    }
    Ok(())
}
