use std::io::Cursor;

use acadrust::document::HeaderVariables;
use acadrust::io::dwg::crc::crc16;
use acadrust::io::dwg::r12::header::{
    read_header, write_auxheader, write_header, HeaderLayout, HeaderReferences, TableLocator,
    BLOCK_ENTITIES_BEGIN, BLOCK_ENTITIES_END, ENTITIES_END, ENTITIES_START, EXTRA_ENTITIES_BEGIN,
    EXTRA_ENTITIES_END, TABLE_BEGIN,
};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfVersion, EntityType, Handle, Vector3};

// Entity bytes are assembled independently of the native encoder. Field
// widths and JUMP's section-relative address/early CRC follow the CC0
// Kaitai Struct schema for the R12 entity stream.
fn frame(kind: u8, flags: u8, options: u16, handle: u8, body: &[u8]) -> Vec<u8> {
    let mut bytes = vec![kind, flags | 32, 0, 0, 0, 0];
    bytes.extend_from_slice(&options.to_le_bytes());
    bytes.extend_from_slice(&[1, handle]);
    bytes.extend_from_slice(body);
    let size = (bytes.len() + 2) as u16;
    bytes[2..4].copy_from_slice(&size.to_le_bytes());
    let crc = crc16(0xc0c1, &bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes
}

fn doubles(values: &[f64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn text(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn jump(address: u32, padding: usize) -> Vec<u8> {
    let mut bytes = vec![18, 0];
    bytes.extend_from_slice(&(10_u16 + padding as u16).to_le_bytes());
    bytes.extend_from_slice(&address.to_le_bytes());
    let crc = crc16(0xc0c1, &bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes.resize(10 + padding, 0xa5);
    bytes
}

fn polyline(handle: u8) -> Vec<u8> {
    frame(19, 128, 0, handle, &[])
}

fn vertex(handle: u8, x: f64, y: f64) -> Vec<u8> {
    frame(20, 0, 0, handle, &doubles(&[x, y]))
}

fn point(handle: u8) -> Vec<u8> {
    frame(2, 0, 0, handle, &doubles(&[3.0, 4.0, 5.0]))
}

fn seqend(handle: u8, owner_address: u32) -> Vec<u8> {
    frame(17, 0, 0, handle, &owner_address.to_le_bytes())
}

fn baseline_dwg() -> Vec<u8> {
    let mut document = CadDocument::with_version(DxfVersion::AC1009);
    document
        .add_entity(EntityType::Line(acadrust::Line::from_coords(
            0.0, 0.0, 0.0, 1.0, 1.0, 0.0,
        )))
        .unwrap();
    DwgWriter::write_to_vec(&document).unwrap()
}

fn block_begin() -> Vec<u8> {
    let mut body = doubles(&[0.0, 0.0]);
    text(&mut body, "PART");
    frame(12, 0, 4, 0x10, &body)
}

fn fixture(entities: &[u8], blocks: &[u8], extras: &[u8], has_part: bool) -> Vec<u8> {
    let layout = HeaderLayout {
        entities_start: ENTITIES_START,
        entities_end: ENTITIES_START + entities.len() as u32,
        blocks_start: ENTITIES_START + entities.len() as u32 + 32,
        blocks_size: blocks.len() as u32,
        extras_start: ENTITIES_START + entities.len() as u32 + blocks.len() as u32 + 64,
        extras_size: extras.len() as u32,
    };
    let mut tables = [TableLocator::default(); 10];
    let mut table_bytes = Vec::new();
    let table_start = layout.extras_start + extras.len() as u32 + 16;
    // Reuse CAD-generated default table records while independently constructing
    // the entity streams. Even empty native tables require valid framing.
    let baseline = baseline_dwg();
    let baseline_header = read_header(&baseline).unwrap();
    for index in 0..10 {
        let source = baseline_header.tables[index];
        let mut rows = baseline[source.address as usize
            ..source.address as usize + source.count as usize * source.record_size as usize]
            .to_vec();
        let mut count = source.count;
        if index == 0 && has_part {
            let mut row = vec![0; 33];
            row[1..5].copy_from_slice(b"PART");
            row.extend_from_slice(&(-1_i16).to_le_bytes());
            row.extend_from_slice(&0x40000000_u32.to_le_bytes());
            row.extend_from_slice(&[0; 4]);
            row.extend_from_slice(&crc16(0xc0c1, &row).to_le_bytes());
            rows = row;
            count = 1;
        }
        tables[index] = TableLocator {
            record_size: source.record_size,
            count,
            flags: 0,
            address: table_start + table_bytes.len() as u32 + 16,
        };
        table_bytes.extend_from_slice(&TABLE_BEGIN[index]);
        table_bytes.extend_from_slice(&rows);
        table_bytes.extend(TABLE_BEGIN[index].iter().map(|byte| !byte));
    }
    let mut variables = HeaderVariables::default();
    variables.handle_seed = 0x1000;
    let references = HeaderReferences {
        ucs_index: -1,
        paper_ucs_index: -1,
        linetype_index: -1,
        ..HeaderReferences::default()
    };
    let mut bytes = write_header(&variables, layout, &tables, references, 0, None).unwrap();
    bytes.extend_from_slice(entities);
    bytes.extend_from_slice(&ENTITIES_END);
    bytes.extend_from_slice(&BLOCK_ENTITIES_BEGIN);
    bytes.extend_from_slice(blocks);
    bytes.extend_from_slice(&BLOCK_ENTITIES_END);
    bytes.extend_from_slice(&EXTRA_ENTITIES_BEGIN);
    bytes.extend_from_slice(extras);
    bytes.extend_from_slice(&EXTRA_ENTITIES_END);
    bytes.extend_from_slice(&table_bytes);
    bytes.extend_from_slice(&write_auxheader(
        layout,
        &tables,
        variables.handle_seed,
        table_start + table_bytes.len() as u32,
    ));
    bytes
}

fn read(bytes: Vec<u8>) -> CadDocument {
    DwgReader::from_stream(Cursor::new(bytes)).read().unwrap()
}

fn assert_polyline(document: &CadDocument, handle: u8, owner: Handle) {
    let EntityType::Polyline2D(polyline) =
        document.get_entity(Handle::new(u64::from(handle))).unwrap()
    else {
        panic!("missing compound POLYLINE");
    };
    assert_eq!(polyline.common.owner_handle, owner);
    let points: Vec<_> = polyline
        .vertices
        .iter()
        .map(|vertex| vertex.location)
        .collect();
    assert_eq!(
        points,
        [Vector3::new(1.0, 2.0, 0.0), Vector3::new(8.0, 9.0, 0.0)]
    );
}

fn assert_stable_saves(mut document: CadDocument, verify: impl Fn(&CadDocument)) {
    for _ in 0..3 {
        verify(&document);
        let first = DwgWriter::write_to_vec(&document).unwrap();
        assert_handseed_exceeds_stored_handles(&first);
        assert_eq!(DwgWriter::write_to_vec(&document).unwrap(), first);
        document = read(first);
    }
    verify(&document);
}

fn assert_handseed_exceeds_stored_handles(bytes: &[u8]) {
    let header = read_header(bytes).unwrap();
    for (start, size) in [
        (
            header.layout.entities_start,
            header.layout.entities_end - header.layout.entities_start,
        ),
        (header.layout.blocks_start, header.layout.blocks_size),
    ] {
        let mut offset = start as usize;
        let end = offset + size as usize;
        while offset < end {
            let record = &bytes[offset..];
            let size = u16::from_le_bytes(record[2..4].try_into().unwrap()) as usize;
            let flags = record[1];
            let mut cursor = 8;
            if flags & 64 != 0 {
                let extra = record[cursor];
                cursor += 1;
                if extra & 2 != 0 {
                    cursor += 2
                        + u16::from_le_bytes(record[cursor..cursor + 2].try_into().unwrap())
                            as usize;
                }
            }
            if flags & 1 != 0 {
                cursor += 1;
            }
            if flags & 2 != 0 {
                cursor += 2;
            }
            if flags & 8 != 0 {
                cursor += 8;
            }
            if flags & 4 != 0 && !matches!(record[0], 1 | 2 | 21 | 22) {
                cursor += 8;
            }
            if flags & 32 != 0 {
                let count = record[cursor] as usize;
                let handle = record[cursor + 1..cursor + 1 + count]
                    .iter()
                    .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte));
                assert!(
                    header.variables.handle_seed > handle,
                    "HANDSEED must exceed generated child handle {handle:X}"
                );
            }
            offset += size;
        }
    }
}

pub fn model_polyline_fixture() -> Vec<u8> {
    let polyline = polyline(0x20);
    let continuation = polyline.len() + 14;
    let entities = [polyline, jump(0x80000000, 4), point(0x30)].concat();
    let extras = [
        vertex(0x21, 1.0, 2.0),
        vertex(0x22, 8.0, 9.0),
        seqend(0x23, ENTITIES_START),
        jump(ENTITIES_START + continuation as u32, 0),
    ]
    .concat();
    fixture(&entities, &[], &extras, false)
}

#[test]
fn model_polyline_follows_padded_jump_and_returns_without_duplicate_children() {
    let document = read(model_polyline_fixture());
    assert_stable_saves(document, |document| {
        assert_eq!(document.entities().count(), 2);
        assert_polyline(document, 0x20, document.header.model_space_block_handle);
        assert!(matches!(
            document.get_entity(Handle::new(0x30)),
            Some(EntityType::Point(_))
        ));
        assert!(document.get_entity(Handle::new(0x21)).is_none());
    });
}

pub fn block_polyline_fixture() -> Vec<u8> {
    let begin = block_begin();
    let owner = ENTITIES_START + point(0x30).len() as u32 + 32 + begin.len() as u32;
    let blocks = [
        begin,
        polyline(0x20),
        vertex(0x21, 1.0, 2.0),
        vertex(0x22, 8.0, 9.0),
        seqend(0x23, owner),
        frame(13, 0, 0, 0x11, &[]),
    ]
    .concat();
    fixture(&point(0x30), &blocks, &[], true)
}

pub fn block_polyline_extra_fixture() -> Vec<u8> {
    let begin = block_begin();
    let polyline = polyline(0x20);
    let to_extra = jump(0x8000_0000, 0);
    let resume = begin.len() + polyline.len() + to_extra.len();
    let blocks = [
        begin.clone(),
        polyline,
        to_extra,
        frame(13, 0, 0, 0x11, &[]),
    ]
    .concat();
    let owner = 0x4000_0000 | begin.len() as u32;
    let extras = [
        vertex(0x21, 1.0, 2.0),
        vertex(0x22, 8.0, 9.0),
        seqend(0x23, owner),
        jump(0x4000_0000 | resume as u32, 0),
    ]
    .concat();
    fixture(&point(0x30), &blocks, &extras, true)
}

#[test]
fn block_polyline_preserves_native_block_ownership_across_saves() {
    let document = read(block_polyline_fixture());
    assert_stable_saves(document, |document| {
        let block = document.block_records.get("PART").unwrap();
        assert_polyline(document, 0x20, block.handle);
        assert_eq!(block.entity_handles, [Handle::new(0x20)]);
        assert_eq!(document.entities().count(), 2);
    });
}

#[test]
fn block_polyline_extra_stream_preserves_ownership_across_saves() {
    let document = read(block_polyline_extra_fixture());
    assert_stable_saves(document, |document| {
        let block = document.block_records.get("PART").unwrap();
        assert_polyline(document, 0x20, block.handle);
        assert_eq!(block.entity_handles, [Handle::new(0x20)]);
        assert_eq!(document.entities().count(), 2);
    });
}

pub fn insert_attributes_fixture() -> Vec<u8> {
    let mut insert_body = 0_u16.to_le_bytes().to_vec();
    insert_body.extend_from_slice(&doubles(&[1.0, 2.0]));
    let insert = frame(14, 128, 0, 0x20, &insert_body);
    let continuation = insert.len() + 10;
    let entities = [insert, jump(0x80000000, 0), point(0x30)].concat();
    let mut body = doubles(&[1.0, 2.0, 3.0]);
    text(&mut body, "value");
    text(&mut body, "TAG");
    body.push(0);
    let extras = [
        frame(16, 0, 0, 0x21, &body),
        seqend(0x22, ENTITIES_START),
        jump(ENTITIES_START + continuation as u32, 0),
    ]
    .concat();
    let blocks = [block_begin(), frame(13, 0, 0, 0x11, &[])].concat();
    fixture(&entities, &blocks, &extras, true)
}

#[test]
fn insert_attributes_continue_in_extras_and_keep_handles_and_owner() {
    let document = read(insert_attributes_fixture());
    assert_stable_saves(document, |document| {
        let EntityType::Insert(insert) = document.get_entity(Handle::new(0x20)).unwrap() else {
            panic!("missing INSERT")
        };
        assert_eq!(insert.attributes.len(), 1);
        assert_eq!(insert.attributes[0].common.handle, Handle::new(0x21));
        assert_eq!(insert.attributes[0].common.owner_handle, Handle::new(0x20));
        assert_eq!(insert.attributes[0].value, "value");
        assert_eq!(insert.seqend_handle, Some(Handle::new(0x22)));
        assert_eq!(document.entities().count(), 2);
    });
}

#[test]
fn invalid_jump_targets_and_owner_addresses_are_rejected_deterministically() {
    for (address, extras, expected) in [
        (0, Vec::new(), "cycle"),
        (0x80000001, point(0x30), "record boundary"),
        (0x8000ffff, point(0x30), "section bounds"),
        (0x20000000, Vec::new(), "section flag"),
    ] {
        let bytes = fixture(&jump(address, 2), &[], &extras, false);
        let first = DwgReader::from_stream(Cursor::new(bytes.clone()))
            .read()
            .unwrap_err()
            .to_string();
        let second = DwgReader::from_stream(Cursor::new(bytes))
            .read()
            .unwrap_err()
            .to_string();
        assert_eq!(first, second);
        assert!(first.contains(expected), "{first}");
    }
    let entities = [polyline(0x20), jump(0x80000000, 0)].concat();
    let extras = [vertex(0x21, 1.0, 2.0), seqend(0x22, ENTITIES_START + 1)].concat();
    let error = DwgReader::from_stream(Cursor::new(fixture(&entities, &[], &extras, false)))
        .read()
        .unwrap_err()
        .to_string();
    assert!(error.contains("SEQEND"), "{error}");
}
