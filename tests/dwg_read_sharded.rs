//! `DwgReader::read_sharded` reads model space in bounded shards that,
//! merged, hold exactly the entities of a single read.
use acadrust::entities::{AttributeEntity, Insert, Line, Polyline3D};
use acadrust::tables::BlockRecord;
use acadrust::types::Vector3;
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfError, EntityType};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Lines, 3D polylines with vertices and inserts with attributes, so shards
/// must keep child records with their owner.
fn drawing(entities: u32) -> CadDocument {
    let mut document = CadDocument::new();
    let mut record = BlockRecord::new("TAG");
    record.handle = document.allocate_handle();
    record.block_entity_handle = document.allocate_handle();
    record.block_end_handle = document.allocate_handle();
    document.block_records.add(record).unwrap();

    for i in 0..entities {
        let x = f64::from(i);
        let entity = match i % 3 {
            0 => EntityType::Line(Line::from_points(
                Vector3::new(x, 0.0, 0.0),
                Vector3::new(x, 1.0, 0.0),
            )),
            1 => {
                let mut polyline = Polyline3D::new();
                for y in 0..4 {
                    polyline.add_vertex(Vector3::new(x, f64::from(y), 0.0));
                }
                EntityType::Polyline3D(polyline)
            }
            _ => {
                let mut insert = Insert::new("TAG", Vector3::new(x, 5.0, 0.0));
                insert.attributes = vec![
                    AttributeEntity::new("A".into(), format!("a{i}")),
                    AttributeEntity::new("B".into(), format!("b{i}")),
                ];
                EntityType::Insert(insert)
            }
        };
        document.add_entity(entity).unwrap();
    }
    document
}

fn read_shards(bytes: &[u8], max_entities: usize) -> Vec<CadDocument> {
    let mut shards = Vec::new();
    let count = DwgReader::from_stream(Cursor::new(bytes))
        .read_sharded(max_entities, |shard| {
            shards.push(shard.document);
            Ok(())
        })
        .unwrap();
    assert_eq!(count, shards.len());
    shards
}

fn model_space(document: &CadDocument) -> BTreeMap<u64, EntityType> {
    document
        .model_space_entities()
        .map(|entity| (entity.common().handle.value(), entity.clone()))
        .collect()
}

/// Every shard's model space, merged; panics if two shards hold one entity.
fn merged(shards: &[CadDocument]) -> BTreeMap<u64, EntityType> {
    let mut merged = BTreeMap::new();
    for shard in shards {
        for (handle, entity) in model_space(shard) {
            assert!(
                merged.insert(handle, entity).is_none(),
                "entity {handle:#X} is in two shards"
            );
        }
    }
    merged
}

#[test]
fn merged_shards_equal_a_single_read() {
    let bytes = DwgWriter::write_to_vec(&drawing(60)).unwrap();
    let single = DwgReader::from_stream(Cursor::new(&bytes)).read().unwrap();

    let shards = read_shards(&bytes, 10);
    assert!(shards.len() > 1, "60 entities must not fit one shard of 10");
    assert_eq!(merged(&shards), model_space(&single));
}

#[test]
fn a_shard_holds_at_most_the_limit_with_children() {
    let bytes = DwgWriter::write_to_vec(&drawing(60)).unwrap();
    for shard in read_shards(&bytes, 10) {
        // Each entity counts with its child records: a polyline's vertices
        // and sequence end, an insert's attributes and sequence end.
        let records: usize = shard
            .model_space_entities()
            .map(|entity| match entity {
                EntityType::Polyline3D(polyline) => 2 + polyline.vertices.len(),
                EntityType::Insert(insert) => 2 + insert.attributes.len(),
                _ => 1,
            })
            .sum();
        assert!(records <= 10, "a shard holds {records} records");
        assert!(shard.model_space_entities().count() > 0);
    }
}

#[test]
fn child_records_stay_with_their_owner() {
    let bytes = DwgWriter::write_to_vec(&drawing(60)).unwrap();
    let shards = read_shards(&bytes, 4);
    let entities = merged(&shards);
    let inserts = entities
        .values()
        .filter_map(|entity| match entity {
            EntityType::Insert(insert) => Some(insert),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(inserts.len(), 20);
    for insert in inserts {
        let tags: Vec<_> = insert.attributes.iter().map(|a| a.tag.as_str()).collect();
        assert_eq!(tags, ["A", "B"]);
    }
    for entity in entities.values() {
        if let EntityType::Polyline3D(polyline) = entity {
            assert_eq!(polyline.vertices.len(), 4);
        }
    }
}

#[test]
fn every_shard_carries_the_tables_and_blocks() {
    let bytes = DwgWriter::write_to_vec(&drawing(60)).unwrap();
    let single = DwgReader::from_stream(Cursor::new(&bytes)).read().unwrap();
    for shard in read_shards(&bytes, 10) {
        assert_eq!(shard.header.handle_seed, single.header.handle_seed);
        assert_eq!(shard.layers.len(), single.layers.len());
        assert_eq!(shard.block_records.len(), single.block_records.len());
        assert!(shard.block_records.get("TAG").is_some());
    }
}

#[test]
fn a_drawing_that_fits_is_one_shard() {
    let bytes = DwgWriter::write_to_vec(&drawing(60)).unwrap();
    let single = DwgReader::from_stream(Cursor::new(&bytes)).read().unwrap();
    let shards = read_shards(&bytes, 1_000);
    assert_eq!(shards.len(), 1);
    assert_eq!(model_space(&shards[0]), model_space(&single));
    assert_eq!(shards[0].objects.len(), single.objects.len());
}

#[test]
fn an_error_from_the_callback_stops_the_read() {
    let bytes = DwgWriter::write_to_vec(&drawing(60)).unwrap();
    let mut calls = 0;
    let result = DwgReader::from_stream(Cursor::new(&bytes)).read_sharded(10, |_| {
        calls += 1;
        Err(DxfError::Parse("stop".into()))
    });
    assert!(result.is_err());
    assert_eq!(calls, 1);
}

fn sample_drawings(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sample_drawings(&path, found);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dwg"))
        {
            found.push(path);
        }
    }
}

#[test]
fn sample_drawings_read_in_shards_equal_a_single_read() {
    let mut paths = Vec::new();
    sample_drawings(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"),
        &mut paths,
    );
    paths.sort();
    let mut split = 0;
    for path in paths {
        let bytes = std::fs::read(&path).unwrap();
        let Ok(single) = DwgReader::from_stream(Cursor::new(&bytes)).read() else {
            continue;
        };
        let shards = read_shards(&bytes, 2);
        if shards.len() > 1 {
            split += 1;
        }
        assert_eq!(merged(&shards), model_space(&single), "{}", path.display());
    }
    assert!(
        split > 0,
        "no sample drawing was read in more than one shard"
    );
}
