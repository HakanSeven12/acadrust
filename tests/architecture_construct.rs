//! Arch construction and optional ACAD fixture checks.
//!
//! Fixture tests are explicitly ignored. Running them with `--ignored` requires
//! the named local drawings; missing files are test failures.

use acadrust::classes::arch::{
    continuous_wall_outline, continuous_walls, set_continuous_wall, set_profile_bounds, set_wall,
    ArchKind,
};
use acadrust::entities::{EntityType, ExtendedEntityData};
use acadrust::types::Vector3;
use acadrust::{DwgReader, DwgWriter};

const DIAGONAL_WALL: &str = r"C:\Users\Hakan\AppData\Local\Temp\aec-made-diag.dwg";

#[test]
#[ignore = "requires the local ACAD-generated aec-made-diag.dwg fixture"]
fn set_wall_rewrites_start_direction_and_size() {
    let path = std::path::Path::new(DIAGONAL_WALL);
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let mut doc = DwgReader::from_file(path).unwrap().read().unwrap();
    let start = Vector3::new(10.0, 20.0, 0.0);
    let end = Vector3::new(10.0, 80.0, 0.0);
    let mut built = false;
    for entity in doc.entities_mut() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &entity.data else {
            continue;
        };
        if data.kind != ArchKind::Wall {
            continue;
        }
        let ExtendedEntityData::Arch(data) = &mut entity.data else {
            continue;
        };
        built = set_wall(data, &mut entity.common.graphic_data, start, end, 8.0, 96.0).is_ok();
        let profile = data.profiles.first().cloned().unwrap_or_default();
        assert!(
            profile
                .iter()
                .any(|point| (point.x - 6.0).abs() < 1e-4 && (point.y - 20.0).abs() < 1e-4),
            "{profile:?}"
        );
        assert!(
            profile
                .iter()
                .any(|point| (point.x - 14.0).abs() < 1e-4 && (point.y - 80.0).abs() < 1e-4),
            "{profile:?}"
        );
        break;
    }
    assert!(built, "diagonal seed wall was not recognized");
    let out = std::env::temp_dir().join("acadrust-constructed-wall.dwg");
    DwgWriter::write_to_file(&out, &doc).unwrap();
}

#[test]
#[ignore = "requires local ACAD Arch Sample Project 2027 drawings"]
fn unsupported_sample_translations_leave_native_data_unchanged() {
    let delta = Vector3::new(100.0, 50.0, 0.0);
    let cases = [
        (
            r"C:\Users\Hakan\Documents\Autodesk\My Projects\Sample Project 2027\Constructs\Architectural\Spaces\01 Spaces.dwg",
            "AEC_SPACE",
        ),
        (
            r"C:\Users\Hakan\Documents\Autodesk\My Projects\Sample Project 2027\Constructs\Architectural\Stairs\01-02 Stairs.dwg",
            "AEC_STAIR",
        ),
        (
            r"C:\Users\Hakan\Documents\Autodesk\My Projects\Sample Project 2027\Constructs\Architectural\Stairs\01-02 Stairs.dwg",
            "AEC_RAILING",
        ),
        (
            r"C:\Users\Hakan\Documents\Autodesk\My Projects\Sample Project 2027\Constructs\Architectural\Shell\01 Masonry Shell.dwg",
            "AEC_DOOR",
        ),
        (
            r"C:\Users\Hakan\Documents\Autodesk\My Projects\Sample Project 2027\Constructs\Architectural\Shell\01 Masonry Shell.dwg",
            "AEC_MASS_ELEM",
        ),
        (
            r"C:\Users\Hakan\Documents\Autodesk\My Projects\Sample Project 2027\Constructs\Structural\Framing\01 Framing.dwg",
            "AECS_MEMBER",
        ),
    ];
    for (path, class_name) in cases {
        assert!(
            std::path::Path::new(path).is_file(),
            "missing fixture: {path}"
        );
        let mut doc = DwgReader::from_file(path).unwrap().read().unwrap();
        let mut found = false;
        for entity in doc.entities_mut() {
            let EntityType::Extended(entity) = entity else {
                continue;
            };
            let ExtendedEntityData::Arch(data) = &mut entity.data else {
                continue;
            };
            if data.dxf_name != class_name {
                continue;
            }
            let before = (data.clone(), entity.common.graphic_data.clone());
            assert!(
                data.translate(&mut entity.common.graphic_data, delta)
                    .is_err(),
                "{class_name} unexpectedly accepted a proxy-only translation"
            );
            assert_eq!(
                (&*data, &entity.common.graphic_data),
                (&before.0, &before.1)
            );
            found = true;
            break;
        }
        assert!(found, "{path} is missing {class_name}");
    }
}

#[test]
#[ignore = "requires the local ACAD Arch Sample Project 2027 space drawing"]
fn unsupported_space_bounds_leave_native_data_unchanged() {
    let path = r"C:\Users\Hakan\Documents\Autodesk\My Projects\Sample Project 2027\Constructs\Architectural\Spaces\01 Spaces.dwg";
    assert!(
        std::path::Path::new(path).is_file(),
        "missing fixture: {path}"
    );
    let mut doc = DwgReader::from_file(path).unwrap().read().unwrap();
    let min = Vector3::new(10.0, 20.0, 0.0);
    let max = Vector3::new(80.0, 60.0, 0.0);
    let mut found = false;
    for entity in doc.entities_mut() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &mut entity.data else {
            continue;
        };
        if data.dxf_name != "AEC_SPACE" {
            continue;
        }
        let before = (data.clone(), entity.common.graphic_data.clone());
        assert!(set_profile_bounds(data, &mut entity.common.graphic_data, min, max).is_err());
        assert_eq!(
            (&*data, &entity.common.graphic_data),
            (&before.0, &before.1)
        );
        found = true;
        break;
    }
    assert!(found, "space fixture is missing AEC_SPACE");
}

#[test]
#[ignore = "requires the manually saved aec-space-moved-acad.dwg fixture"]
fn autocad_keeps_a_moved_space() {
    let path = std::env::temp_dir().join("aec-space-moved-acad.dwg");
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let first = doc
        .entities()
        .find_map(|entity| {
            let EntityType::Extended(entity) = entity else {
                return None;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                return None;
            };
            (data.dxf_name == "AEC_SPACE").then(|| {
                data.profiles
                    .first()
                    .and_then(|profile| profile.first())
                    .copied()
            })
        })
        .flatten()
        .expect("space");
    assert!((first.x - -34.0).abs() < 1e-3, "{first:?}");
    assert!((first.y - 373.375).abs() < 1e-3, "{first:?}");
}

#[test]
#[ignore = "requires the manually saved acadrust-constructed-wall-acad.dwg fixture"]
fn autocad_keeps_the_rewritten_vertical_wall() {
    let path = std::env::temp_dir().join("acadrust-constructed-wall-acad.dwg");
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let wall = doc
        .entities()
        .find_map(|entity| {
            let EntityType::Extended(entity) = entity else {
                return None;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                return None;
            };
            (data.kind == ArchKind::Wall).then_some(data.profiles.first().cloned())
        })
        .flatten()
        .expect("wall");
    assert!(
        wall.iter()
            .any(|point| (point.x - 6.0).abs() < 1e-3 && (point.y - 20.0).abs() < 1e-3),
        "ACAD rebuilt a different wall: {wall:?}"
    );
    assert!(
        wall.iter()
            .any(|point| (point.x - 14.0).abs() < 1e-3 && (point.y - 80.0).abs() < 1e-3),
        "ACAD rebuilt a different wall: {wall:?}"
    );
}

#[test]
#[ignore = "requires the manually saved acadrust-vertical-wall-acad.dwg fixture"]
fn autocad_keeps_the_native_vertical_wall_baseline() {
    let path = std::env::temp_dir().join("acadrust-vertical-wall-acad.dwg");
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let wall = doc
        .entities()
        .find_map(|entity| {
            let EntityType::Extended(entity) = entity else {
                return None;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                return None;
            };
            data.straight_wall_geometry()
        })
        .expect("vertical wall baseline");
    assert!(
        (wall.start.x - 25000.0).abs() < 1.0 && (wall.start.y - 21000.0).abs() < 1.0,
        "start {:?}",
        wall.start
    );
    assert!(
        (wall.end.x - 25000.0).abs() < 1.0 && (wall.end.y - 15000.0).abs() < 1.0,
        "end {:?}",
        wall.end
    );
    assert!((wall.width - 300.0).abs() < 1.0, "width {}", wall.width);
    assert!((wall.height - 3200.0).abs() < 1.0, "height {}", wall.height);
}

fn count_class(doc: &acadrust::CadDocument, class_name: &str) -> usize {
    doc.entities()
        .filter(|entity| {
            matches!(
                entity,
                EntityType::Extended(entity)
                    if matches!(&entity.data, ExtendedEntityData::Arch(data) if data.dxf_name == class_name)
            )
        })
        .count()
}

fn wall_handles(doc: &acadrust::CadDocument) -> Vec<acadrust::types::Handle> {
    doc.entities()
        .filter_map(|entity| {
            let EntityType::Extended(entity) = entity else {
                return None;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                return None;
            };
            (data.dxf_name == "AEC_WALL").then_some(entity.common.handle)
        })
        .collect()
}

#[test]
#[ignore = "requires AEC SAMPLE DRAWINGS/WALL_VERTICAL_PLUS_DOOR.dwg"]
fn removing_a_door_leaves_its_wall() {
    let path = sample_dir().join("WALL_VERTICAL_PLUS_DOOR.dwg");
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let mut doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let door = doc.entities().find_map(|entity| {
        let EntityType::Extended(entity) = entity else {
            return None;
        };
        let ExtendedEntityData::Arch(data) = &entity.data else {
            return None;
        };
        (data.dxf_name == "AEC_DOOR").then_some(entity.common.handle)
    });
    let door = door.expect("door fixture must contain AEC_DOOR");
    doc.remove_entity(door);
    assert_eq!(count_class(&doc, "AEC_DOOR"), 0);
    assert_eq!(count_class(&doc, "AEC_WALL"), 1);
}

#[test]
#[ignore = "requires the local AEC SAMPLE DRAWINGS directory"]
fn removing_a_wall_removes_openings_anchored_to_it() {
    let mut files: Vec<_> = std::fs::read_dir(sample_dir())
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dwg"))
        })
        .collect();
    files.sort();
    let mut checked = 0usize;
    for path in files {
        let file_name = path.file_name().unwrap().to_string_lossy();
        let mut doc = DwgReader::from_file(&path).unwrap().read().unwrap();
        let walls = wall_handles(&doc);
        if walls.is_empty() {
            continue;
        }
        let hosted_before = [
            "AEC_DOOR",
            "AEC_WINDOW",
            "AEC_OPENING",
            "AEC_WINDOW_ASSEMBLY",
        ]
        .into_iter()
        .map(|name| count_class(&doc, name))
        .sum::<usize>();
        if hosted_before == 0 {
            continue;
        }
        checked += 1;
        for wall in walls {
            doc.remove_entity(wall);
        }
        assert_eq!(count_class(&doc, "AEC_WALL"), 0, "{file_name}");
        let hosted_after = [
            "AEC_DOOR",
            "AEC_WINDOW",
            "AEC_OPENING",
            "AEC_WINDOW_ASSEMBLY",
        ]
        .into_iter()
        .map(|name| count_class(&doc, name))
        .sum::<usize>();
        assert_eq!(hosted_after, 0, "{file_name} left {hosted_after} openings");
        assert!(
            doc.objects.values().all(|object| {
                !matches!(
                    object,
                    acadrust::objects::ObjectType::Arch(object)
                        if object.dxf_name.contains("ANCHOR_OPENINGBASE_TO_WALL")
                )
            }),
            "{file_name} left an opening anchor"
        );
    }
    assert!(
        checked > 0,
        "no drawings containing hosted openings were checked"
    );
}

#[test]
#[ignore = "requires AEC SAMPLE DRAWINGS/COUNTINIOUS_IRREGULAR_WALL.dwg"]
fn continuous_irregular_wall_is_one_chain_and_can_move() {
    let path = sample_dir().join("COUNTINIOUS_IRREGULAR_WALL.dwg");
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let mut doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let chains = continuous_walls(&doc);
    assert_eq!(chains.len(), 1, "{chains:?}");
    assert_eq!(chains[0].handles.len(), 3);
    assert_eq!(chains[0].points.len(), 4);
    assert!((chains[0].width - 240.0).abs() < 0.1, "{}", chains[0].width);
    assert!(
        (chains[0].height - 3000.0).abs() < 0.1,
        "{}",
        chains[0].height
    );
    let outline = continuous_wall_outline(&chains[0].points, chains[0].width).unwrap();
    assert!(outline.len() >= 8, "{outline:?}");
    let delta = Vector3::new(1000.0, -500.0, 0.0);
    let moved: Vec<Vector3> = chains[0]
        .points
        .iter()
        .map(|point| *point + delta)
        .collect();
    assert!(set_continuous_wall(&mut doc, &chains[0], &moved, 240.0, 3000.0).is_ok());
    let rewritten = continuous_walls(&doc);
    assert_eq!(rewritten.len(), 1);
    assert!((rewritten[0].points[0] - moved[0]).length() < 1.0);
    assert!((rewritten[0].points[3] - moved[3]).length() < 1.0);
    DwgWriter::write_to_file(
        std::env::temp_dir().join("acadrust-continuous-wall.dwg"),
        &doc,
    )
    .unwrap();
}

#[test]
#[ignore = "requires the manually saved acadrust-continuous-wall-acad.dwg fixture"]
fn autocad_keeps_a_moved_continuous_wall() {
    let path = std::env::temp_dir().join("acadrust-continuous-wall-acad.dwg");
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let chains = continuous_walls(&doc);
    assert_eq!(chains.len(), 1, "{chains:?}");
    let expected = [
        Vector3::new(28329.0, 32345.0, 0.0),
        Vector3::new(17823.0, 10165.0, 0.0),
        Vector3::new(51813.0, 4504.0, 0.0),
        Vector3::new(72413.0, 27199.0, 0.0),
    ];
    assert_eq!(chains[0].points.len(), expected.len());
    for (actual, expected) in chains[0].points.iter().zip(expected) {
        assert!(
            (*actual - expected).length() < 2.0,
            "continuous wall moved to {actual:?}, expected {expected:?}, chain {:?}",
            chains[0].points
        );
    }
}

#[test]
fn wall_door_and_window_can_be_created_without_an_input_drawing() {
    let wall = acadrust::classes::arch::wall(
        Vector3::new(25000.0, 15000.0, 0.0),
        Vector3::new(25000.0, 21000.0, 0.0),
        300.0,
        3200.0,
        None,
    )
    .unwrap();
    assert_eq!(count_class(&wall, "AEC_WALL"), 1);
    assert_eq!(count_class(&wall, "AEC_DOOR"), 0);
    assert_eq!(count_class(&wall, "AEC_WINDOW"), 0);
    let horizontal = acadrust::classes::arch::wall(
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(8000.0, 0.0, 0.0),
        240.0,
        3000.0,
        None,
    )
    .unwrap();
    assert_eq!(count_class(&horizontal, "AEC_WALL"), 1);
    let geometry = read_first(&horizontal, "AEC_WALL")
        .and_then(|data| data.straight_wall_geometry())
        .expect("horizontal wall");
    assert!(geometry.start.x.abs() < 1.0 && geometry.start.y.abs() < 1.0);
    assert!((geometry.end.x - 8000.0).abs() < 1.0 && geometry.end.y.abs() < 1.0);
    assert!((geometry.width - 240.0).abs() < 1.0);
    let door = acadrust::classes::arch::door(900.0, 2100.0, None).unwrap();
    assert_eq!(count_class(&door, "AEC_WALL"), 0);
    assert_eq!(count_class(&door, "AEC_DOOR"), 1);
    let door_entity = read_first(&door, "AEC_DOOR").unwrap();
    let size = acadrust::classes::arch::door_size(&door_entity).unwrap();
    assert!((size.width - 900.0).abs() < 1.0, "{}", size.width);
    assert!((size.height - 2100.0).abs() < 1.0, "{}", size.height);
    let window = acadrust::classes::arch::window(1200.0, 1500.0, None).unwrap();
    assert_eq!(count_class(&window, "AEC_WALL"), 0);
    assert_eq!(count_class(&window, "AEC_WINDOW"), 1);
    let window_entity = read_first(&window, "AEC_WINDOW").unwrap();
    let size = acadrust::classes::arch::window_size(&window_entity).unwrap();
    assert!((size.width - 1200.0).abs() < 1.0, "{}", size.width);
    assert!((size.height - 1500.0).abs() < 1.0, "{}", size.height);
}

#[test]
fn any_constructed_aec_class_decodes_without_a_private_schema() {
    let wall = acadrust::classes::arch::wall(
        Vector3::new(10.0, 20.0, 0.0),
        Vector3::new(8010.0, 20.0, 0.0),
        240.0,
        3000.0,
        None,
    )
    .unwrap();
    let door = acadrust::classes::arch::door(900.0, 2100.0, None).unwrap();
    let window = acadrust::classes::arch::window(1200.0, 1500.0, None).unwrap();
    for (document, class_name, schema) in [
        (wall, "AEC_WALL", 569),
        (door, "AEC_DOOR", 558),
        (window, "AEC_WINDOW", 559),
    ] {
        let data = read_first(&document, class_name).unwrap();
        let decoded = data.decode();
        assert_eq!(decoded.dxf_name, class_name);
        assert!(decoded.is_entity);
        assert_eq!(decoded.schema, Some(schema));
        let placement = decoded.placement.expect("entity placement");
        assert!(placement.rigid, "{class_name} {placement:?}");
        assert!(!decoded.strings.is_empty(), "{class_name}");
        assert!(!decoded.handles.is_empty(), "{class_name}");
    }
    let wall = acadrust::classes::arch::wall(
        Vector3::new(10.0, 20.0, 0.0),
        Vector3::new(8010.0, 20.0, 0.0),
        240.0,
        3000.0,
        None,
    )
    .unwrap();
    let style = wall.objects.values().find_map(|object| {
        let acadrust::objects::ObjectType::Arch(object) = object else {
            return None;
        };
        (object.dxf_name == "AEC_WALL_STYLE").then_some(object.decode())
    });
    let style = style.expect("wall style");
    assert_eq!(style.schema, Some(547));
    assert!(style.placement.is_none());
    assert!(style.strings.iter().any(|text| text == "Unnamed"));
}

#[test]
#[ignore = "requires the local AEC SAMPLE DRAWINGS directory"]
fn sample_aec_records_share_one_envelope() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("AEC SAMPLE DRAWINGS");
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dwg"))
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "missing fixtures in {}", root.display());
    let mut entities = 0usize;
    let mut objects = 0usize;
    for path in files {
        let doc = DwgReader::from_file(&path).unwrap().read().unwrap();
        for entity in doc.entities() {
            let EntityType::Extended(entity) = entity else {
                continue;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                continue;
            };
            entities += 1;
            let decoded = data.decode();
            assert_eq!(decoded.dxf_name, data.dxf_name, "{}", path.display());
            assert!(
                decoded.schema.is_some(),
                "{} {} has no AEC tag",
                path.display(),
                data.dxf_name
            );
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let straight_fixture = matches!(
                file_name,
                "WALL_VERTICAL.dwg"
                    | "WALL_VERTICAL_PLUS_DOOR.dwg"
                    | "WALL_VERTICAL_PLUS_WINDOW.dwg"
                    | "HORIZONTAL_STAIR.dwg"
                    | "SPACE_RECTANGLE.dwg"
                    | "SLAB.dwg"
            );
            if straight_fixture
                && matches!(
                    data.kind,
                    ArchKind::Wall
                        | ArchKind::Door
                        | ArchKind::Window
                        | ArchKind::Stair
                        | ArchKind::Space
                        | ArchKind::Slab
                )
            {
                assert!(
                    decoded.placement.is_some(),
                    "{} {} has no placement",
                    path.display(),
                    data.dxf_name
                );
            }
        }
        for object in doc.objects.values() {
            let acadrust::objects::ObjectType::Arch(object) = object else {
                continue;
            };
            objects += 1;
            let decoded = object.decode();
            assert_eq!(decoded.dxf_name, object.dxf_name);
            assert!(
                decoded.schema.is_some(),
                "{} {} has no AEC tag",
                path.display(),
                object.dxf_name
            );
            assert!(decoded.placement.is_none());
        }
    }
    assert!(entities > 10, "entities {entities}");
    assert!(objects > 10, "objects {objects}");
}

#[test]
fn constructors_use_autocad_architecture_layers_unless_one_is_requested() {
    let cases = [
        (
            acadrust::classes::arch::wall(
                Vector3::ZERO,
                Vector3::new(1000.0, 0.0, 0.0),
                240.0,
                3000.0,
                None,
            )
            .unwrap(),
            "AEC_WALL",
            "A-Wall",
            113,
            50,
        ),
        (
            acadrust::classes::arch::door(900.0, 2100.0, None).unwrap(),
            "AEC_DOOR",
            "A-Door",
            31,
            25,
        ),
        (
            acadrust::classes::arch::window(1200.0, 1500.0, None).unwrap(),
            "AEC_WINDOW",
            "A-Glaz",
            151,
            25,
        ),
    ];
    for (document, class_name, layer_name, color, line_weight) in cases {
        let layer = document.layers.get(layer_name).expect(layer_name);
        assert_eq!(layer.color, acadrust::types::Color::from_index(color));
        assert_eq!(
            layer.line_weight,
            acadrust::types::LineWeight::from_value(line_weight)
        );
        assert_eq!(document.header.current_layer_name, layer_name);
        let entity_layer = document.entities().find_map(|entity| {
            let EntityType::Extended(entity) = entity else {
                return None;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                return None;
            };
            (data.dxf_name == class_name).then_some(entity.common.layer.clone())
        });
        assert_eq!(entity_layer.as_deref(), Some(layer_name));
        let bytes = DwgWriter::write_to_vec(&document).unwrap();
        let reread = DwgReader::from_stream(std::io::Cursor::new(bytes))
            .read()
            .unwrap();
        assert!(reread.layers.get(layer_name).is_some(), "{layer_name}");
        let reread_layer = reread.entities().find_map(|entity| {
            let EntityType::Extended(entity) = entity else {
                return None;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                return None;
            };
            (data.dxf_name == class_name).then_some(entity.common.layer.clone())
        });
        assert_eq!(reread_layer.as_deref(), Some(layer_name));
    }

    let custom = acadrust::classes::arch::wall(
        Vector3::ZERO,
        Vector3::new(500.0, 0.0, 0.0),
        200.0,
        2700.0,
        Some("Partitions"),
    )
    .unwrap();
    assert_eq!(custom.header.current_layer_name, "Partitions");
    assert!(custom.layers.get("A-Wall").is_none());
    assert_eq!(
        custom.layers.get("Partitions").unwrap().color,
        acadrust::types::Color::WHITE
    );
    let known = acadrust::classes::arch::door(800.0, 2000.0, Some("a-glaz")).unwrap();
    let glazing = known.layers.get("A-Glaz").unwrap();
    assert_eq!(glazing.color, acadrust::types::Color::from_index(151));
    assert!(acadrust::classes::arch::window(1000.0, 1000.0, Some("   ")).is_err());
}

#[test]
fn architecture_area_group_has_a_distinct_kind() {
    assert_eq!(
        acadrust::classes::arch::kind_of("AEC_AREA"),
        Some(ArchKind::Area)
    );
    assert_eq!(
        acadrust::classes::arch::kind_of("AEC_AREAGROUP"),
        Some(ArchKind::AreaGroup)
    );
}

#[test]
fn constructed_entities_survive_dwg_serialization_without_fixture_state() {
    let expected_start = Vector3::new(-120.0, 50.0, 0.0);
    let expected_end = Vector3::new(2880.0, 4050.0, 0.0);
    let documents = [
        (
            "AEC_WALL",
            acadrust::classes::arch::wall(expected_start, expected_end, 175.0, 2800.0, None)
                .unwrap(),
        ),
        (
            "AEC_DOOR",
            acadrust::classes::arch::door(2600.0, 2100.0, None).unwrap(),
        ),
        (
            "AEC_WINDOW",
            acadrust::classes::arch::window(1800.0, 2400.0, None).unwrap(),
        ),
    ];
    for (class_name, document) in documents {
        let (schema, location, direction) = match class_name {
            "AEC_WALL" => (
                569,
                expected_start,
                (expected_end - expected_start).normalize(),
            ),
            "AEC_DOOR" => (558, Vector3::ZERO, Vector3::UNIT_X),
            "AEC_WINDOW" => (559, Vector3::ZERO, Vector3::UNIT_X),
            _ => unreachable!(),
        };
        assert_native_entity_prefix_and_self_reference(
            &document, class_name, schema, location, direction,
        );
        let bytes = DwgWriter::write_to_vec(&document).unwrap();
        let reread = DwgReader::from_stream(std::io::Cursor::new(bytes))
            .read()
            .unwrap();
        assert_eq!(reread.entities().count(), 1, "{class_name}");
        assert_eq!(count_class(&reread, class_name), 1, "{class_name}");
        let data = read_first(&reread, class_name).unwrap();
        match class_name {
            "AEC_WALL" => {
                let geometry = data.straight_wall_geometry().expect("native wall fields");
                assert!((geometry.start - expected_start).length() < 1e-6);
                assert!((geometry.end - expected_end).length() < 1e-6);
                assert!((geometry.width - 175.0).abs() < 1e-6);
                assert!((geometry.height - 2800.0).abs() < 1e-6);
            }
            "AEC_DOOR" => {
                let size = acadrust::classes::arch::door_size(&data).expect("native door fields");
                assert_eq!((size.width, size.height), (2600.0, 2100.0));
            }
            "AEC_WINDOW" => {
                let size =
                    acadrust::classes::arch::window_size(&data).expect("native window fields");
                assert_eq!((size.width, size.height), (1800.0, 2400.0));
            }
            _ => unreachable!(),
        }
    }
}

fn assert_native_entity_prefix_and_self_reference(
    document: &acadrust::CadDocument,
    class_name: &str,
    schema: i16,
    location: Vector3,
    direction: Vector3,
) {
    let (entity_handle, data) = document
        .entities()
        .find_map(|entity| {
            let EntityType::Extended(entity) = entity else {
                return None;
            };
            let ExtendedEntityData::Arch(data) = &entity.data else {
                return None;
            };
            (data.dxf_name == class_name).then_some((entity.common.handle, data))
        })
        .unwrap_or_else(|| panic!("missing {class_name}"));
    let raw = data.raw_dwg_data.as_ref().expect("native entity body");
    let mut record = Vec::from((raw.len() as u16).to_le_bytes());
    let mut handle_bits = data.dwg_handle_bits as u64;
    loop {
        let byte = (handle_bits & 0x7f) as u8;
        handle_bits >>= 7;
        record.push(byte | if handle_bits > 0 { 0x80 } else { 0 });
        if handle_bits == 0 {
            break;
        }
    }
    record.extend(raw);

    let object_reader = acadrust::io::dwg::dwg_stream_readers::object_reader::DwgObjectReader::new(
        record,
        document.version,
        Default::default(),
    )
    .unwrap();
    let (type_code, mut reader) = object_reader.read_record_at(0).unwrap();
    let common = object_reader.read_common_entity_data(&mut reader, type_code);
    assert_eq!(common.common.handle, entity_handle.value());

    assert_eq!(reader.read_bit_short(), 80);
    assert!(reader.read_bit());
    assert!(reader.read_bit());
    assert_eq!(reader.read_bit_short(), schema);
    assert_eq!(reader.read_bit_short(), 80);
    assert_eq!(reader.read_variable_text(), "");
    assert_eq!(reader.read_bit_short(), 80);
    assert_eq!(reader.read_bit_short(), 0);
    assert_eq!(reader.read_byte(), 1);
    assert!(!reader.read_bit());
    assert_eq!(reader.read_bit_short(), 80);
    assert_eq!(reader.read_variable_text(), "");

    let near = |left: Vector3, right: Vector3| (left - right).length() < 1e-9;
    assert!(near(reader.read_3bit_double(), location));
    assert!(near(reader.read_3bit_double(), direction));
    assert!(near(
        reader.read_3bit_double(),
        Vector3::new(-direction.y, direction.x, 0.0)
    ));
    assert!(near(reader.read_3bit_double(), Vector3::UNIT_Z));

    let mut handles = Vec::new();
    while reader.handle_remaining_bits() >= 8 {
        handles.push(reader.read_typed_handle());
    }
    assert_eq!(
        handles.last(),
        Some(&(
            entity_handle.value(),
            acadrust::io::dwg::dwg_reference_type::DwgReferenceType::SoftPointer
        )),
        "{class_name} must end with its native self-reference"
    );
}

#[test]
fn constructed_aec_styles_match_native_filefiler_body_ends() {
    let documents = [
        (
            "AEC_WALL_STYLE",
            595,
            140,
            acadrust::classes::arch::wall(
                Vector3::ZERO,
                Vector3::new(8000.0, 0.0, 0.0),
                240.0,
                3000.0,
                None,
            )
            .unwrap(),
        ),
        (
            "AEC_DOOR_STYLE",
            515,
            4,
            acadrust::classes::arch::door(900.0, 2100.0, None).unwrap(),
        ),
        (
            "AEC_WINDOW_STYLE",
            453,
            4,
            acadrust::classes::arch::window(1200.0, 1500.0, None).unwrap(),
        ),
    ];
    let tail = "1101001011010101010000010101000001000000011101110001000101010000010101000001010100001001010100000101010000";

    for (class_name, body_bits, text_bits, document) in documents {
        let object = document
            .objects
            .values()
            .find_map(|object| match object {
                acadrust::objects::ObjectType::Arch(object) if object.dxf_name == class_name => {
                    Some(object)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {class_name}"));
        let raw = object.raw_dwg_data.as_ref().expect("native style body");
        let mut record = Vec::from((raw.len() as u16).to_le_bytes());
        let mut handle_bits = object.raw_dwg_handle_bits as u64;
        loop {
            let byte = (handle_bits & 0x7f) as u8;
            handle_bits >>= 7;
            record.push(byte | if handle_bits > 0 { 0x80 } else { 0 });
            if handle_bits == 0 {
                break;
            }
        }
        record.extend(raw);

        let reader = acadrust::io::dwg::dwg_stream_readers::object_reader::DwgObjectReader::new(
            record,
            document.version,
            Default::default(),
        )
        .unwrap();
        let (type_code, mut data) = reader.read_record_at(0).unwrap();
        reader.read_common_non_entity_data(&mut data, type_code);
        assert_eq!(data.read_bit_short(), 80);
        assert!(data.read_bit());
        assert!(data.read_bit());
        data.read_bit_short();
        assert_eq!(data.read_bit_short(), 80);
        assert_eq!(data.read_variable_text(), "");
        assert_eq!(data.read_bit_short(), 80);
        assert_eq!(data.read_variable_text(), "");
        assert!(!data.read_bit());
        assert_eq!(data.read_byte(), 0);
        assert_eq!(data.read_bit_short(), 80);

        let body_start = data.position_in_bits();
        assert_eq!(data.main_remaining_bits(), body_bits, "{class_name}");
        assert_eq!(data.text_remaining_bits(), text_bits, "{class_name}");
        data.set_position_in_bits(body_start + body_bits - tail.len() as i64);
        let actual_tail: String = (0..tail.len())
            .map(|_| if data.read_bit() { '1' } else { '0' })
            .collect();
        assert_eq!(actual_tail, tail, "{class_name}");
        if text_bits == 4 {
            assert_eq!(data.read_variable_text(), "");
            assert_eq!(data.read_variable_text(), "");
            assert_eq!(data.text_remaining_bits(), 0, "{class_name}");
        }
    }
}

#[test]
fn constructor_parameters_are_validated_before_encoding() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(acadrust::classes::arch::door(bad, 2100.0, None).is_err());
        assert!(acadrust::classes::arch::door(900.0, bad, None).is_err());
        assert!(acadrust::classes::arch::window(bad, 1500.0, None).is_err());
        assert!(acadrust::classes::arch::window(1200.0, bad, None).is_err());
        assert!(
            acadrust::classes::arch::wall(Vector3::ZERO, Vector3::UNIT_X, bad, 3000.0, None)
                .is_err()
        );
        assert!(
            acadrust::classes::arch::wall(Vector3::ZERO, Vector3::UNIT_X, 240.0, bad, None)
                .is_err()
        );
    }
    assert!(
        acadrust::classes::arch::wall(Vector3::ZERO, Vector3::ZERO, 240.0, 3000.0, None).is_err()
    );
    assert!(acadrust::classes::arch::wall(
        Vector3::ZERO,
        Vector3::new(1.0, 0.0, 1.0),
        240.0,
        3000.0,
        None,
    )
    .is_err());
    assert!(acadrust::classes::arch::wall(
        Vector3::new(f64::NAN, 0.0, 0.0),
        Vector3::UNIT_X,
        240.0,
        3000.0,
        None,
    )
    .is_err());
}

fn read_first(
    doc: &acadrust::CadDocument,
    class_name: &str,
) -> Option<acadrust::classes::arch::ArchEntityData> {
    doc.entities().find_map(|entity| {
        let EntityType::Extended(entity) = entity else {
            return None;
        };
        let ExtendedEntityData::Arch(data) = &entity.data else {
            return None;
        };
        (data.dxf_name == class_name).then(|| data.clone())
    })
}

#[test]
#[ignore = "requires local AEC SAMPLE DRAWINGS fixtures"]
fn deciphered_sizes_match_the_samples_and_can_be_rewritten() {
    {
        let mut data = read_class("WALL_VERTICAL_PLUS_DOOR.dwg", "AEC_DOOR");
        let size = acadrust::classes::arch::door_size(&data).expect("door");
        assert!((size.width - 1010.0).abs() < 1.0, "{}", size.width);
        assert!((size.height - 2000.0).abs() < 1.0, "{}", size.height);
        let mut graphics = None;
        assert!(acadrust::classes::arch::set_door(&mut data, &mut graphics, 900.0, 2100.0).is_ok());
        let updated = acadrust::classes::arch::door_size(&data).unwrap();
        assert!((updated.width - 900.0).abs() < 1.0);
        assert!((updated.height - 2100.0).abs() < 1.0);
        let mut loaded = read_class("WALL_VERTICAL_PLUS_DOOR.dwg", "AEC_DOOR");
        loaded.decode_fields();
        assert!(loaded.fields.iter().any(|field| {
            field.role == acadrust::classes::arch::NumberRole::Height
                && (field.value - 2000.0).abs() < 1.0
        }));
        assert!(loaded.fields.iter().any(|field| {
            field.role == acadrust::classes::arch::NumberRole::Width
                && (field.value - 1010.0).abs() < 1.0
        }));
        assert!(!loaded.preview.is_empty() || loaded.profiles.is_empty());
    }
    {
        let data = read_class("WALL_VERTICAL_PLUS_WINDOW.dwg", "AEC_WINDOW");
        let size = acadrust::classes::arch::window_size(&data).expect("window");
        assert!((size.width - 1010.0).abs() < 1.0, "{}", size.width);
        // The 1000-unit native frame elevation is the sill, not the height.
        assert!((size.height - 1510.0).abs() < 1.0, "{}", size.height);
    }
    {
        let data = read_class("WALL_VERTICAL_PLUS_OPENING.dwg", "AEC_OPENING");
        let size = acadrust::classes::arch::opening_size(&data).expect("opening");
        assert!((size.width - 1000.0).abs() < 1.0, "{}", size.width);
        assert!((size.height - 2000.0).abs() < 1.0, "{}", size.height);
    }
    {
        let data = read_class("HORIZONTAL_STAIR.dwg", "AEC_STAIR");
        let size = acadrust::classes::arch::stair_size(&data).expect("stair");
        assert!((size.run - 4320.0).abs() < 1.0, "{}", size.run);
        assert!((size.width - 1000.0).abs() < 1.0, "{}", size.width);
        assert!((size.height - 3000.0).abs() < 1.0, "{}", size.height);
    }
    {
        let data = read_class("BEAM.dwg", "AECS_MEMBER");
        let size = acadrust::classes::arch::member_size(&data).expect("beam");
        assert!((size.length - 3047.0).abs() < 1.0, "{}", size.length);
        assert!((size.breadth - 300.0).abs() < 1.0, "{}", size.breadth);
        assert!((size.depth - 300.0).abs() < 1.0, "{}", size.depth);
    }
    {
        let data = read_class("COLUMN.dwg", "AECS_MEMBER");
        let size = acadrust::classes::arch::member_size(&data).expect("column");
        assert!((size.length - 3000.0).abs() < 1.0, "{}", size.length);
        assert!((size.breadth - 300.0).abs() < 1.0 && (size.depth - 300.0).abs() < 1.0);
    }
}

fn read_class(file_name: &str, class_name: &str) -> acadrust::classes::arch::ArchEntityData {
    let path = sample_dir().join(file_name);
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    let found = doc.entities().find_map(|entity| {
        let EntityType::Extended(entity) = entity else {
            return None;
        };
        let ExtendedEntityData::Arch(data) = &entity.data else {
            return None;
        };
        (data.dxf_name == class_name).then(|| data.clone())
    });
    found.unwrap_or_else(|| panic!("{} is missing {class_name}", path.display()))
}

fn sample_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("AEC SAMPLE DRAWINGS")
}

#[test]
#[ignore = "requires AEC SAMPLE DRAWINGS/WALL_VERTICAL.dwg"]
fn vertical_sample_wall_accepts_a_new_baseline() {
    let path = sample_dir().join("WALL_VERTICAL.dwg");
    assert!(path.is_file(), "missing fixture: {}", path.display());
    let mut doc = DwgReader::from_file(&path).unwrap().read().unwrap();
    // Keep the sample's downward normal; reversing it resizes a compact field.
    let start = Vector3::new(25000.0, 21000.0, 0.0);
    let end = Vector3::new(25000.0, 15000.0, 0.0);
    let mut built = false;
    for entity in doc.entities_mut() {
        let EntityType::Extended(entity) = entity else {
            continue;
        };
        let ExtendedEntityData::Arch(data) = &mut entity.data else {
            continue;
        };
        if data.dxf_name != "AEC_WALL" {
            continue;
        }
        built = set_wall(
            data,
            &mut entity.common.graphic_data,
            start,
            end,
            300.0,
            3200.0,
        )
        .is_ok();
        let profile = data.profiles.first().cloned().unwrap_or_default();
        assert!(
            profile
                .iter()
                .any(|point| (point.x - 24850.0).abs() < 1.0 && (point.y - 15000.0).abs() < 1.0),
            "{profile:?}"
        );
        assert!(
            profile
                .iter()
                .any(|point| (point.x - 25150.0).abs() < 1.0 && (point.y - 21000.0).abs() < 1.0),
            "{profile:?}"
        );
        break;
    }
    assert!(built, "vertical wall fields were not found");
    DwgWriter::write_to_file(
        std::env::temp_dir().join("acadrust-vertical-wall.dwg"),
        &doc,
    )
    .unwrap();
}

#[test]
#[ignore = "requires local AEC SAMPLE DRAWINGS fixtures"]
fn unsupported_sample_bounds_leave_native_data_unchanged() {
    let cases = [
        ("SPACE_RECTANGLE.dwg", "AEC_SPACE"),
        ("HORIZONTAL_STAIR.dwg", "AEC_STAIR"),
        ("HORIZONTAL_RAILING.dwg", "AEC_RAILING"),
        ("SLAB.dwg", "AEC_SLAB"),
        ("ROOF_SLAB.dwg", "AEC_ROOFSLAB"),
        ("ROOF.dwg", "AEC_ROOF"),
        ("BEAM.dwg", "AECS_MEMBER"),
        ("COLUMN.dwg", "AECS_MEMBER"),
        ("CURTAIN_WALL_VERTICAL.dwg", "AEC_CURTAIN_WALL_LAYOUT"),
        ("COLUMN_GRID.dwg", "AEC_COLUMN_GRID"),
        ("WALL_VERTICAL_PLUS_DOOR.dwg", "AEC_DOOR"),
        ("WALL_VERTICAL_PLUS_WINDOW.dwg", "AEC_WINDOW"),
        ("WALL_VERTICAL_PLUS_OPENING.dwg", "AEC_OPENING"),
        ("DISPLAY_THEME.dwg", "AEC_DISPLAYTHEME"),
        ("SPACE_INSERT.dwg", "AEC_SPACE"),
        ("SPACE_POLYGONAL.dwg", "AEC_SPACE"),
    ];
    let min = Vector3::new(1000.0, 2000.0, 0.0);
    let max = Vector3::new(4000.0, 6500.0, 0.0);
    for (file_name, class_name) in cases {
        let path = sample_dir().join(file_name);
        assert!(path.is_file(), "missing fixture: {}", path.display());
        let mut doc = DwgReader::from_file(&path).unwrap().read().unwrap();
        let mut found = false;
        for entity in doc.entities_mut() {
            let EntityType::Extended(entity) = entity else {
                continue;
            };
            let ExtendedEntityData::Arch(data) = &mut entity.data else {
                continue;
            };
            if data.dxf_name != class_name {
                continue;
            }
            let before = (data.clone(), entity.common.graphic_data.clone());
            assert!(
                set_profile_bounds(data, &mut entity.common.graphic_data, min, max).is_err(),
                "{file_name} {class_name} unexpectedly accepted proxy-only bounds"
            );
            assert_eq!(
                (&*data, &entity.common.graphic_data),
                (&before.0, &before.1)
            );
            found = true;
            break;
        }
        assert!(found, "{file_name} is missing {class_name}");
    }
}
