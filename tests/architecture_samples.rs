//! Optional checks against local ACAD Arch project drawings.
//! Run with `--ignored` after supplying the named fixtures.

use acadrust::classes::arch::{is_architecture_name, reshape_rectangle, ArchKind};
use acadrust::entities::{EntityType, ExtendedEntityData};
use acadrust::objects::ObjectType;
use acadrust::types::Vector3;
use acadrust::{DwgReader, DwgWriter};

const SHELL: &str = r"C:\Users\Hakan\Documents\Autodesk\My Projects\Sample Project 2027\Constructs\Architectural\Shell\01 Masonry Shell.dwg";

fn shell_path() -> std::path::PathBuf {
    let path = std::path::PathBuf::from(SHELL);
    assert!(path.is_file(), "missing fixture: {}", path.display());
    path
}

#[test]
#[ignore = "requires the local ACAD Arch Sample Project 2027 masonry shell"]
fn masonry_shell_walls_round_trip_after_move() {
    let path = shell_path();
    let mut doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let mut walls = 0usize;
    let mut styled = 0usize;
    for object in doc.objects.values() {
        if let ObjectType::Arch(object) = object {
            assert!(is_architecture_name(&object.dxf_name));
            assert!(object.raw_dwg_data.is_some());
            styled += 1;
        }
    }
    assert!(styled > 100, "style and display objects: {styled}");

    let delta = Vector3::new(120.0, -40.0, 0.0);
    let mut moved = None;
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
        walls += 1;
        assert!(!data.profiles.is_empty(), "{}", data.dxf_name);
        assert!(data.profiles[0].len() >= 2);
        if moved.is_none() {
            let before = data.profiles[0][0];
            assert!(data
                .translate(&mut entity.common.graphic_data, delta)
                .is_ok());
            moved = Some((entity.common.handle, before + delta, data.profiles[0][0]));
        }
    }
    assert!(walls >= 10, "walls: {walls}");
    let (handle, expected, stored) = moved.unwrap();
    assert_eq!(stored, expected);

    let out = std::env::temp_dir().join("acadrust-arch-shell.dwg");
    DwgWriter::write_to_file(&out, &doc).unwrap();
    let reread = DwgReader::from_file(&out).unwrap().read().unwrap();
    let entity = reread.get_entity(handle).expect("moved wall");
    let EntityType::Extended(entity) = entity else {
        panic!("wall was not kept as an Arch entity");
    };
    let ExtendedEntityData::Arch(data) = &entity.data else {
        panic!("wall class payload missing");
    };
    assert_eq!(data.dxf_name, "AEC_WALL");
    assert_eq!(data.kind, ArchKind::Wall);
    assert_eq!(data.profiles[0][0], expected);
}

#[test]
#[ignore = "requires the local ACAD Arch Sample Project 2027 masonry shell"]
fn masonry_shell_wall_rejects_preview_only_reshape() {
    let path = shell_path();
    let mut doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let start = Vector3::new(0.0, 0.0, 0.0);
    let end = Vector3::new(240.0, 0.0, 0.0);
    let mut checked = false;
    for entity in doc.entities_mut() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &entity.data else {
            continue;
        };
        if data.kind != ArchKind::Wall
            || data.profiles.first().map(|profile| profile.len()) != Some(5)
        {
            continue;
        }
        let ExtendedEntityData::Arch(data) = &mut entity.data else {
            continue;
        };
        let before = data.clone();
        let graphics_before = entity.common.graphic_data.clone();
        assert!(reshape_rectangle(data, &mut entity.common.graphic_data, start, end, 8.0).is_err());
        assert_eq!(*data, before);
        assert_eq!(entity.common.graphic_data, graphics_before);
        checked = true;
        break;
    }
    assert!(checked, "no 5-point wall profile to check");
}

#[test]
#[ignore = "requires the manually saved acadrust-arch-moved-acad.dwg fixture"]
fn autocad_keeps_architecture_entities_after_save() {
    let path = std::env::temp_dir().join("acadrust-arch-moved-acad.dwg");
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let mut walls = 0usize;
    let mut doors = 0usize;
    let mut assemblies = 0usize;
    for entity in doc.entities() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &entity.data else {
            continue;
        };
        match data.kind {
            ArchKind::Wall => walls += 1,
            ArchKind::Door => doors += 1,
            ArchKind::WindowAssembly => assemblies += 1,
            _ => {}
        }
    }
    assert!(walls >= 10, "walls {walls}");
    assert!(doors >= 1, "doors {doors}");
    assert!(assemblies >= 1, "window assemblies {assemblies}");
}
