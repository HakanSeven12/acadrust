use acadrust::classes::arch::*;
use acadrust::entities::{EntityType, ExtendedEntityData};
use acadrust::io::dwg::dwg_stream_readers::object_reader::DwgObjectReader;
use acadrust::types::{Handle, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter};

fn v(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x, y, z)
}

fn sample(file: &str) -> CadDocument {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("AEC SAMPLE DRAWINGS")
        .join(file);
    DwgReader::from_file(path).unwrap().read().unwrap()
}

fn first(doc: &CadDocument, kind: ArchKind) -> (ArchEntityData, Option<Vec<u8>>) {
    doc.entities()
        .find_map(|e| {
            let EntityType::Extended(e) = e else {
                return None;
            };
            let ExtendedEntityData::Arch(data) = &e.data else {
                return None;
            };
            (data.kind == kind).then(|| (data.clone(), e.common.graphic_data.clone()))
        })
        .unwrap()
}

fn spans(data: &ArchEntityData) -> Vector3 {
    let mut min = v(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut max = v(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in data.profiles.iter().flatten() {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        min.z = min.z.min(p.z);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
        max.z = max.z.max(p.z);
    }
    max - min
}

fn agrees(data: &ArchEntityData, graphics: &Option<Vec<u8>>) {
    let raw = data.raw_dwg_data.as_ref().unwrap();
    assert!(raw.len() < 0x8000);
    let mut framed = (raw.len() as u16).to_le_bytes().to_vec();
    let mut remaining = data.dwg_handle_bits as u64;
    loop {
        let byte = (remaining & 0x7f) as u8;
        remaining >>= 7;
        framed.push(byte | if remaining != 0 { 0x80 } else { 0 });
        if remaining == 0 {
            break;
        }
    }
    framed.extend_from_slice(raw);
    let objects =
        DwgObjectReader::new(framed, data.dwg_source_version.unwrap(), Default::default()).unwrap();
    let (kind, mut reader) = objects.read_record_at(0).unwrap();
    let common = objects.read_common_entity_data(&mut reader, kind);
    reader.check_error().unwrap();
    assert_eq!(
        decode_profiles(common.graphic_data.as_deref()),
        data.profiles
    );
    if let Some(graphics) = graphics {
        assert_eq!(decode_profiles(Some(graphics)), data.profiles)
    }
    assert!(data.fields.iter().all(|field| field.value.is_finite()));
}

#[test]
fn zero_coordinate_wall_translation_preserves_native_record_and_dwg_handles() {
    let mut document = wall(v(0.0, 0.0, 0.0), v(8000.0, 0.0, 0.0), 240.0, 3000.0, None).unwrap();
    let mut changed = false;
    for e in document.entities_mut() {
        let EntityType::Extended(e) = e else { continue };
        let ExtendedEntityData::Arch(data) = &mut e.data else {
            continue;
        };
        if data.kind != ArchKind::Wall {
            continue;
        }
        assert!(data
            .translate(&mut e.common.graphic_data, v(10.0, 20.0, 0.0))
            .is_ok());
        let geometry = data.straight_wall_geometry().unwrap();
        assert!((geometry.start - v(10.0, 20.0, 0.0)).length() < 1e-6);
        agrees(data, &e.common.graphic_data);
        changed = true;
    }
    assert!(changed);
    let path =
        std::env::temp_dir().join(format!("acadrust-aec-geometry-{}.dwg", std::process::id()));
    DwgWriter::write_to_file(&path, &document).unwrap();
    let reread = DwgReader::from_file(&path).unwrap().read().unwrap();
    let (data, graphics) = first(&reread, ArchKind::Wall);
    assert!((data.straight_wall_geometry().unwrap().start - v(10.0, 20.0, 0.0)).length() < 1e-6);
    agrees(&data, &graphics);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn translating_by_an_existing_coordinate_does_not_cascade() {
    let document = wall(
        v(240.0, 0.0, 0.0),
        v(240.0, 6000.0, 0.0),
        240.0,
        3000.0,
        None,
    )
    .unwrap();
    let (mut data, mut graphics) = first(&document, ArchKind::Wall);
    let before = data.profiles.clone();
    assert!(data.translate(&mut graphics, v(240.0, 0.0, 0.0)).is_ok());
    for (before, after) in before.iter().flatten().zip(data.profiles.iter().flatten()) {
        assert!((*after - *before - v(240.0, 0.0, 0.0)).length() < 1e-6);
    }
    agrees(&data, &graphics);
}

#[test]
fn opening_sizes_survive_repeated_edits_and_refresh_all_caches() {
    for (document, kind, setter, reader) in [
        (
            door(1010.0, 2000.0, None).unwrap(),
            ArchKind::Door,
            set_door
                as fn(
                    &mut ArchEntityData,
                    &mut Option<Vec<u8>>,
                    f64,
                    f64,
                ) -> Result<(), acadrust::classes::arch::ArchEditError>,
            door_size as fn(&ArchEntityData) -> Option<OpeningSize>,
        ),
        (
            window(1010.0, 1510.0, None).unwrap(),
            ArchKind::Window,
            set_window,
            window_size,
        ),
    ] {
        let (mut data, mut graphics) = first(&document, kind);
        for (width, height) in [
            (900.0, 2100.0),
            (950.0, 2200.0),
            (3000.0, 4500.0),
            (250.0, 800.0),
        ] {
            assert!(
                setter(&mut data, &mut graphics, width, height).is_ok(),
                "{kind:?} {width} {height}"
            );
            let size = reader(&data).unwrap();
            assert_eq!((size.width, size.height), (width, height));
            agrees(&data, &graphics);
            assert!(data
                .fields
                .iter()
                .any(|f| f.role == NumberRole::Width && f.value == width));
        }
        graphics = None;
        assert!(setter(&mut data, &mut graphics, 900.0, 2000.0).is_ok());
        agrees(&data, &graphics);
    }
}

#[test]
fn constructor_fields_encoded_as_zero_or_one_remain_editable() {
    let document = wall(v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 1.0, 1.0, None).unwrap();
    let (mut data, mut graphics) = first(&document, ArchKind::Wall);
    assert!(set_wall(
        &mut data,
        &mut graphics,
        v(0.0, 0.0, 0.0),
        v(2.0, 0.0, 0.0),
        2.0,
        3.0
    )
    .is_ok());
    let wall = data.straight_wall_geometry().unwrap();
    assert_eq!(
        ((wall.end - wall.start).length(), wall.width, wall.height),
        (2.0, 2.0, 3.0)
    );

    let (mut data, mut graphics) = first(&door(1.0, 1.0, None).unwrap(), ArchKind::Door);
    assert_eq!(door_size(&data).unwrap().width, 1.0);
    assert!(set_door(&mut data, &mut graphics, 2.0, 3.0).is_ok());
    let size = door_size(&data).unwrap();
    assert_eq!((size.width, size.height), (2.0, 3.0));

    let (mut data, mut graphics) = first(&window(1.0, 1.0, None).unwrap(), ArchKind::Window);
    assert_eq!(window_size(&data).unwrap().width, 1.0);
    assert!(set_window(&mut data, &mut graphics, 2.0, 3.0).is_ok());
    let size = window_size(&data).unwrap();
    assert_eq!((size.width, size.height), (2.0, 3.0));
}

#[test]
fn wall_baseline_turns_horizontal_and_diagonal() {
    let document = wall(
        v(25000.0, 15000.0, 0.0),
        v(25000.0, 21000.0, 0.0),
        300.0,
        3200.0,
        None,
    )
    .unwrap();
    let (mut data, mut graphics) = first(&document, ArchKind::Wall);
    let horizontal = (
        v(1000.0, 2000.0, 0.0),
        v(9000.0, 2000.0, 0.0),
        240.0,
        3000.0,
    );
    let diagonal = (v(-120.0, 50.0, 0.0), v(2880.0, 4050.0, 0.0), 175.0, 2800.0);
    for (start, end, width, height) in [horizontal, diagonal] {
        assert!(set_wall(&mut data, &mut graphics, start, end, width, height).is_ok());
        let geometry = data.straight_wall_geometry().unwrap();
        assert!((geometry.start - start).length() < 1e-6);
        assert!((geometry.end - end).length() < 1e-6);
        assert!((geometry.width - width).abs() < 1e-6);
        assert!((geometry.height - height).abs() < 1e-6);
        assert!(
            ribbon_matches_baseline(&data, start, end, width),
            "{:?}",
            data.profiles
        );
        agrees(&data, &graphics);
    }
    let bytes = {
        let mut document = document;
        let entity = document.entities_mut().next().unwrap();
        let EntityType::Extended(entity) = entity else {
            panic!("wall");
        };
        entity.data = ExtendedEntityData::Arch(data);
        entity.common.graphic_data = graphics;
        DwgWriter::write_to_vec(&document).unwrap()
    };
    let reread = DwgReader::from_stream(std::io::Cursor::new(bytes))
        .read()
        .unwrap();
    let (data, _) = first(&reread, ArchKind::Wall);
    let geometry = data.straight_wall_geometry().unwrap();
    assert!((geometry.start - diagonal.0).length() < 1e-6);
    assert!((geometry.end - diagonal.1).length() < 1e-6);
}

fn ribbon_matches_baseline(
    data: &ArchEntityData,
    start: Vector3,
    end: Vector3,
    width: f64,
) -> bool {
    let edge = end - start;
    let length = edge.length();
    let direction = edge * (1.0 / length);
    let left = v(-direction.y, direction.x, 0.0);
    data.profiles.first().is_some_and(|profile| {
        profile.len() == 5
            && profile.iter().take(4).all(|point| {
                let relative = *point - start;
                let along = relative.dot(&direction);
                let across = relative.dot(&left);
                (-1e-6..=length + 1e-6).contains(&along)
                    && (across.abs() - width * 0.5).abs() < 1e-4
            })
    })
}

#[test]
#[ignore = "requires AEC SAMPLE DRAWINGS/HORIZONTAL_STAIR.dwg"]
fn stair_edits_reject_incomplete_native_dimension_updates() {
    let (mut data, mut graphics) = first(&sample("HORIZONTAL_STAIR.dwg"), ArchKind::Stair);
    let original = (data.clone(), graphics.clone());
    for (run, width, height) in [
        (5000.0, 1200.0, 3200.0),
        (800.0, 1800.0, 3400.0),
        (600.0, 2200.0, 2600.0),
    ] {
        assert!(set_stair(&mut data, &mut graphics, run, width, height).is_err());
        assert_eq!((data.clone(), graphics.clone()), original);
    }
}

#[test]
#[ignore = "requires local AEC SAMPLE DRAWINGS member fixtures"]
fn member_lengths_follow_native_axis_and_sections_fail_unchanged() {
    let (mut data, mut graphics) = first(&sample("COLUMN.dwg"), ArchKind::StructuralMember);
    assert!(set_member(&mut data, &mut graphics, 4000.0, 300.0, 300.0).is_ok());
    assert!((spans(&data) - v(300.0, 300.0, 4000.0)).length() < 1e-6);
    let original = (data.clone(), graphics.clone());
    assert!(set_member(&mut data, &mut graphics, 3500.0, 600.0, 900.0).is_err());
    assert_eq!((data.clone(), graphics.clone()), original);
    agrees(&data, &graphics);
    let (mut data, mut graphics) = first(&sample("BEAM.dwg"), ArchKind::StructuralMember);
    let origin_y = data
        .profiles
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(set_member(&mut data, &mut graphics, 5000.0, 300.0, 300.0).is_ok());
    assert!((spans(&data) - v(300.0, 5000.0, 300.0)).length() < 1e-6);
    assert_eq!(
        data.profiles
            .iter()
            .flatten()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max),
        origin_y
    );
    agrees(&data, &graphics);
}

#[test]
#[ignore = "requires AEC SAMPLE DRAWINGS/WALL_VERTICAL_PLUS_WINDOW.dwg"]
fn window_height_is_distinct_from_its_sill_elevation() {
    let (mut data, mut graphics) =
        first(&sample("WALL_VERTICAL_PLUS_WINDOW.dwg"), ArchKind::Window);
    assert_eq!(window_size(&data).unwrap().height, 1510.0);
    let sill_before: Vec<_> = data
        .profiles
        .iter()
        .flatten()
        .map(|point| point.z)
        .collect();
    assert!(set_window(&mut data, &mut graphics, 1200.0, 1800.0).is_ok());
    assert_eq!(window_size(&data).unwrap().height, 1800.0);
    assert_eq!(
        data.profiles
            .iter()
            .flatten()
            .map(|point| point.z)
            .collect::<Vec<_>>(),
        sill_before
    );
    agrees(&data, &graphics);
}

#[test]
fn rejected_geometry_changes_leave_every_cache_and_byte_unchanged() {
    let (mut data, mut graphics) = first(&door(1010.0, 2000.0, None).unwrap(), ArchKind::Door);
    for (width, height) in [(f64::NAN, 2000.0), (900.0, f64::INFINITY), (0.0, 2000.0)] {
        let original = (data.clone(), graphics.clone());
        assert!(set_door(&mut data, &mut graphics, width, height).is_err());
        assert_eq!((data.clone(), graphics.clone()), original);
    }
    let original = (data.clone(), graphics.clone());
    assert!(data.translate(&mut graphics, v(10.0, 20.0, 0.0)).is_err());
    assert!(set_profile_bounds(
        &mut data,
        &mut graphics,
        v(0.0, 0.0, 0.0),
        v(5000.0, 5000.0, 0.0)
    )
    .is_err());
    assert_eq!((data.clone(), graphics.clone()), original);
    graphics.as_mut().unwrap()[0] ^= 1;
    let original = (data.clone(), graphics.clone());
    assert!(set_door(&mut data, &mut graphics, 900.0, 2100.0).is_err());
    assert_eq!((data, graphics), original);
}

#[test]
#[ignore = "requires local AEC SAMPLE DRAWINGS slab and beam fixtures"]
fn unsupported_bounds_and_beam_edits_fail_without_changes() {
    let (mut data, mut graphics) = first(&sample("SLAB.dwg"), ArchKind::Slab);
    let original = (data.clone(), graphics.clone());
    assert!(set_slab(
        &mut data,
        &mut graphics,
        v(0.0, 0.0, 0.0),
        v(5000.0, 3000.0, 0.0)
    )
    .is_err());
    assert_eq!((data, graphics), original);
    let (mut data, mut graphics) = first(&sample("BEAM.dwg"), ArchKind::StructuralMember);
    let original = (data.clone(), graphics.clone());
    assert!(set_member(&mut data, &mut graphics, 5000.0, 600.0, 900.0).is_err());
    assert_eq!((data, graphics), original);
}

#[test]
fn continuous_wall_edits_preflight_all_handles_and_dimensions() {
    let mut doc = wall(v(0.0, 0.0, 0.0), v(1000.0, 0.0, 0.0), 240.0, 3000.0, None).unwrap();
    let original = doc.entities().next().unwrap().clone();
    for (start, end) in [
        (v(1000.0, 0.0, 0.0), v(1000.0, 2000.0, 0.0)),
        (v(1000.0, 2000.0, 0.0), v(4000.0, 2000.0, 0.0)),
    ] {
        let mut entity = original.clone();
        entity.common_mut().handle = Handle::NULL;
        let EntityType::Extended(extended) = &mut entity else {
            panic!()
        };
        let ExtendedEntityData::Arch(data) = &mut extended.data else {
            panic!()
        };
        assert!(set_wall(
            data,
            &mut extended.common.graphic_data,
            start,
            end,
            240.0,
            3000.0
        )
        .is_ok());
        doc.add_entity(entity).unwrap();
    }
    let chain = continuous_walls(&doc).remove(0);
    let original: Vec<_> = doc.entities().cloned().collect();
    let points: Vec<_> = chain
        .points
        .iter()
        .map(|point| *point + v(240.0, 0.0, 0.0))
        .collect();
    let mut invalid = chain.clone();
    *invalid.handles.last_mut().unwrap() = Handle::new(u64::MAX);
    assert!(set_continuous_wall(&mut doc, &invalid, &points, 240.0, 3000.0).is_err());
    assert_eq!(doc.entities().cloned().collect::<Vec<_>>(), original);
    invalid = chain.clone();
    invalid.handles[1] = invalid.handles[0];
    assert!(set_continuous_wall(&mut doc, &invalid, &points, 240.0, 3000.0).is_err());
    assert_eq!(doc.entities().cloned().collect::<Vec<_>>(), original);
    assert!(set_continuous_wall(&mut doc, &chain, &points, f64::NAN, 3000.0).is_err());
    assert!(set_continuous_wall(&mut doc, &chain, &points, 240.0, 3000.0).is_ok());
    assert!(continuous_wall_outline(&[v(0.0, 0.0, 0.0), v(1.0, 2.0, f64::NAN)], 240.0).is_none());
    assert!(continuous_wall_outline(&[v(0.0, 0.0, 0.0), v(1.0, 2.0, 3.0)], 240.0).is_none());
}

#[test]
fn continuous_walls_do_not_join_segments_across_a_subunit_gap() {
    let mut doc = wall(v(0.0, 0.0, 0.0), v(1000.0, 0.0, 0.0), 240.0, 3000.0, None).unwrap();
    let mut entity = doc.entities().next().unwrap().clone();
    entity.common_mut().handle = Handle::NULL;
    let EntityType::Extended(extended) = &mut entity else {
        panic!()
    };
    let ExtendedEntityData::Arch(data) = &mut extended.data else {
        panic!()
    };
    assert!(set_wall(
        data,
        &mut extended.common.graphic_data,
        v(1000.5, 0.0, 0.0),
        v(2000.5, 0.0, 0.0),
        240.0,
        3000.0
    )
    .is_ok());
    doc.add_entity(entity).unwrap();
    assert_eq!(continuous_walls(&doc).len(), 2);
}
