//! Entities of a layout other than the active one keep their owner through
//! DWG: only the active layout's *Paper_Space is implied by paper-space mode.
use acadrust::entities::Line;
use acadrust::types::Vector3;
use acadrust::{CadDocument, DwgReader, DwgWriter, EntityType};
use std::io::Cursor;

#[test]
fn a_second_layouts_block_marker_and_entities_keep_their_record() {
    let mut source = CadDocument::new();
    source.add_layout("Layout2").unwrap();
    let line = source
        .add_entity_to_layout(
            EntityType::Line(Line::from_points(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(10.0, 0.0, 0.0),
            )),
            "Layout2",
        )
        .unwrap();

    let bytes = DwgWriter::write_to_vec(&source).unwrap();
    let loaded = DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
    let extra = loaded.block_records.get("*Paper_Space0").unwrap();
    let Some(EntityType::Block(marker)) = loaded.get_entity(extra.block_entity_handle) else {
        panic!("the second layout's BLOCK marker is missing");
    };
    assert_eq!(marker.name, extra.name);
    assert_eq!(marker.common.owner_handle, extra.handle);
    assert_eq!(
        loaded.get_entity(line).unwrap().common().owner_handle,
        extra.handle
    );
    assert!(extra.entity_handles.contains(&line));
}

fn via_dxf(document: &CadDocument) -> CadDocument {
    let bytes = acadrust::DxfWriter::new(document).write_to_vec().unwrap();
    acadrust::DxfReader::from_reader(Cursor::new(bytes))
        .unwrap()
        .read()
        .unwrap()
}

fn via_dwg(document: &CadDocument) -> CadDocument {
    let bytes = DwgWriter::write_to_vec(document).unwrap();
    DwgReader::from_stream(Cursor::new(bytes)).read().unwrap()
}

fn line(x: f64) -> EntityType {
    EntityType::Line(Line::from_points(
        Vector3::new(x, 0.0, 0.0),
        Vector3::new(x + 10.0, 5.0, 0.0),
    ))
}

/// #119: a paper-space INSERT's ATTRIBs carry the DXF paper flag (67 = 1);
/// DWG must still write them as owned by their INSERT.
#[test]
fn paper_space_insert_attributes_survive_dxf_to_dwg() {
    use acadrust::entities::{AttributeEntity, Insert};
    use acadrust::tables::BlockRecord;

    let mut source = CadDocument::new();
    let mut record = BlockRecord::new("K");
    record.handle = source.allocate_handle();
    record.block_entity_handle = source.allocate_handle();
    record.block_end_handle = source.allocate_handle();
    source.block_records.add(record).unwrap();

    let mut insert = Insert::new("K", Vector3::new(5.0, 5.0, 0.0));
    insert.attributes = vec![
        AttributeEntity::new("A".into(), "one".into()),
        AttributeEntity::new("B".into(), "two".into()),
    ];
    let model = source
        .add_entity(EntityType::Insert(insert.clone()))
        .unwrap();
    let paper = source
        .add_entity_to_layout(EntityType::Insert(insert), "Layout1")
        .unwrap();

    let from_dxf = via_dxf(&source);
    let attributes = |document: &CadDocument, handle| match document.get_entity(handle) {
        Some(EntityType::Insert(insert)) => insert.attributes.clone(),
        other => panic!("INSERT missing: {other:?}"),
    };
    let paper_attributes = attributes(&from_dxf, paper);
    assert!(
        paper_attributes
            .iter()
            .all(|a| a.common.entity_mode == Some(1)),
        "the DXF repro must carry the paper flag on its ATTRIBs"
    );

    let back = via_dwg(&from_dxf);
    for handle in [model, paper] {
        let tags: Vec<_> = attributes(&back, handle)
            .into_iter()
            .map(|a| a.tag)
            .collect();
        assert_eq!(tags, ["A", "B"]);
    }
}

/// #123: only the active layout's `*Paper_Space` gets the implicit paper
/// mode; an inactive `*Paper_SpaceN` keeps mode 0 and an explicit owner.
#[test]
fn inactive_layout_entities_use_explicit_ownership() {
    let mut source = CadDocument::new();
    source.add_layout("Layout2").unwrap();
    let model = source.add_entity(line(0.0)).unwrap();
    let active = source.add_entity_to_layout(line(20.0), "Layout1").unwrap();
    let inactive = source.add_entity_to_layout(line(40.0), "Layout2").unwrap();

    let from_dxf = via_dxf(&source);
    let back = via_dwg(&from_dxf);
    for document in [&from_dxf, &back] {
        let record = |name| document.block_records.get(name).unwrap().handle;
        for (handle, owner, mode) in [
            (model, record("*Model_Space"), 2),
            (active, record("*Paper_Space"), 1),
            (inactive, record("*Paper_Space0"), 0),
        ] {
            let common = document.get_entity(handle).unwrap().common();
            assert_eq!(common.owner_handle, owner);
            assert_eq!(common.entity_mode, Some(mode));
        }
    }
}
