//! Leader hook flag and the DXF default for group 74.
//!
//! Issue #176: R2000+ DWG does not store the hook, and a missing group 74 is
//! same-direction (1). AutoCAD omits 74 when the hook runs with the horizontal
//! vector.

use std::io::Cursor;

use acadrust::entities::{EntityType, HooklineDirection, Leader, LeaderCreationType};
use acadrust::types::{DxfVersion, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfWriter};

fn read_dxf(text: &str) -> CadDocument {
    DxfReader::from_reader(Cursor::new(text.as_bytes().to_vec()))
        .expect("open dxf")
        .read()
        .expect("read dxf")
}

fn with_entities(entities: &str) -> CadDocument {
    let text = String::from_utf8(DxfWriter::new(&CadDocument::new()).write_to_vec().unwrap())
        .unwrap()
        .replace("\r\n", "\n");
    let at = text.find("ENTITIES\n").expect("entities section") + "ENTITIES\n".len();
    read_dxf(&format!("{}{entities}{}", &text[..at], &text[at..]))
}

fn leader_of(document: &CadDocument) -> &Leader {
    document
        .entities()
        .find_map(|entity| match entity {
            EntityType::Leader(leader) => Some(leader),
            _ => None,
        })
        .expect("leader")
}

fn round_trip(version: DxfVersion, leader: Leader) -> Leader {
    let mut document = CadDocument::with_version(version);
    document
        .add_entity(EntityType::Leader(leader))
        .expect("add leader");
    let bytes = DwgWriter::write_to_vec(&document).expect("write dwg");
    let loaded = DwgReader::from_stream(Cursor::new(bytes))
        .read()
        .expect("read dwg");
    leader_of(&loaded).clone()
}

fn leader_ending_at(degrees: f64) -> Leader {
    let radians = degrees.to_radians();
    Leader::two_point(
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(100.0 * radians.cos(), 100.0 * radians.sin(), 0.0),
    )
}

#[test]
fn r2000_dwg_derives_the_text_leader_hook() {
    let hooked = round_trip(DxfVersion::AC1032, leader_ending_at(45.0));
    assert!(hooked.hookline_enabled);
    assert_eq!(hooked.vertices.len(), 2);

    let flat = round_trip(DxfVersion::AC1032, leader_ending_at(0.0));
    assert!(!flat.hookline_enabled);

    let mut spline = leader_ending_at(45.0);
    spline.path_type = acadrust::entities::LeaderPathType::Spline;
    assert!(!round_trip(DxfVersion::AC1032, spline).hookline_enabled);
}

#[test]
fn r14_dwg_keeps_the_stored_hook_bit() {
    let mut stored = leader_ending_at(0.0);
    stored.hookline_enabled = true;
    assert!(round_trip(DxfVersion::AC1014, stored).hookline_enabled);

    let mut clear = leader_ending_at(45.0);
    clear.hookline_enabled = false;
    clear.creation_type = LeaderCreationType::WithText;
    assert!(!round_trip(DxfVersion::AC1014, clear).hookline_enabled);
}

#[test]
fn dxf_omitted_group_74_is_same_direction() {
    let leader = "\
  0\nLEADER\n  5\nFF\n100\nAcDbEntity\n  8\n0\n100\nAcDbLeader\n  3\nStandard\n\
 71\n1\n 72\n0\n 73\n0\n 75\n1\n 40\n2.5\n 41\n0.0\n 76\n2\n\
 10\n0.0\n 20\n0.0\n 30\n0.0\n 10\n10.0\n 20\n10.0\n 30\n0.0\n\
210\n0.0\n220\n0.0\n230\n1.0\n211\n1.0\n221\n0.0\n231\n0.0\n";
    let document = with_entities(leader);
    let loaded = leader_of(&document);
    assert_eq!(loaded.hookline_direction, HooklineDirection::Same);
    assert!(loaded.hookline_enabled);

    let opposite = leader.replace(" 75\n1\n", " 74\n0\n 75\n1\n");
    let document = with_entities(&opposite);
    let loaded = leader_of(&document);
    assert_eq!(loaded.hookline_direction, HooklineDirection::Opposite);
}
