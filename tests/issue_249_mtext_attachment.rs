//! Issue #240 regression (#249): MTEXT attachment taken from the embedded object's group 70
//! where it disagrees with group 71 (ODA-written DXF).
//!
//! An ODA-written DXF (such as ODA File Converter's conversion of ACadSharp's sample drawings)
//! contains an MTEXT entity whose group 71 is the actual attachment point (e.g. 6, MiddleRight),
//! while the 101 Embedded Object block carries group 70 = 1 (TopLeft).
//! Group 71 must take precedence over the embedded object's group 70.
//! When group 71 is absent, group 70 serves as the fallback.

use std::io::Cursor;

use acadrust::entities::{AttachmentPoint, EntityType, MText};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader};

/// Build a DXF containing an MTEXT entity with the specified entity group 71
/// and optional embedded object group 70 and column layout.
fn make_mtext_dxf(group_71: Option<i16>, embedded_group_70: Option<i16>) -> Vec<u8> {
    let mut s = String::from(
        "0\r\nSECTION\r\n2\r\nHEADER\r\n9\r\n$ACADVER\r\n1\r\nAC1032\r\n0\r\nENDSEC\r\n\
         0\r\nSECTION\r\n2\r\nENTITIES\r\n0\r\nMTEXT\r\n5\r\n3F6\r\n100\r\nAcDbEntity\r\n\
         8\r\n0\r\n100\r\nAcDbMText\r\n10\r\n199.33\r\n20\r\n15.32\r\n30\r\n0.0\r\n\
         40\r\n1.0\r\n41\r\n15.74\r\n",
    );
    if let Some(ap) = group_71 {
        s.push_str(&format!("71\r\n{ap}\r\n"));
    }
    s.push_str("72\r\n1\r\n1\r\nthis is a Mtext^Jwith multiple lines in it\r\n");
    if let Some(emb_70) = embedded_group_70 {
        s.push_str(&format!(
            "101\r\nEmbedded Object\r\n70\r\n{emb_70}\r\n\
             10\r\n1.0\r\n20\r\n0.0\r\n30\r\n0.0\r\n\
             11\r\n199.33\r\n21\r\n15.32\r\n31\r\n0.0\r\n\
             40\r\n15.741\r\n41\r\n0.0\r\n42\r\n13.495\r\n43\r\n-1.0\r\n\
             71\r\n2\r\n72\r\n1\r\n44\r\n15.741\r\n45\r\n110.22\r\n73\r\n0\r\n74\r\n0\r\n46\r\n0.0\r\n"
        ));
    }
    s.push_str("0\r\nENDSEC\r\n0\r\nEOF\r\n");
    s.into_bytes()
}

fn only_mtext(doc: &CadDocument) -> MText {
    let mut found = doc.entities().filter_map(|e| match e {
        EntityType::MText(m) => Some((**m).clone()),
        _ => None,
    });
    let m = found.next().expect("one MTEXT");
    assert!(found.next().is_none(), "exactly one MTEXT");
    m
}

fn read_dxf(bytes: Vec<u8>) -> CadDocument {
    DxfReader::from_reader(Cursor::new(bytes))
        .unwrap()
        .read()
        .unwrap()
}

#[test]
fn group_71_takes_precedence_over_embedded_object_group_70() {
    // Exactly reproducing issue #249: group 71 is 6 (MiddleRight),
    // embedded object has group 70 = 1 (TopLeft) with dynamic columns.
    let dxf = make_mtext_dxf(Some(6), Some(1));
    let doc = read_dxf(dxf);
    let m = only_mtext(&doc);
    assert_eq!(m.attachment_point, AttachmentPoint::MiddleRight);
    assert_eq!(m.column_data.column_type, 2);
}

#[test]
fn group_71_precedence_survives_dwg_export() {
    // Issue #249: when exported to DWG, the entity's attachment point must remain
    // MiddleRight (6), matching the original DWG rather than TopLeft (1).
    let dxf = make_mtext_dxf(Some(6), Some(1));
    let doc = read_dxf(dxf);
    let bytes = DwgWriter::write_to_vec(&doc).unwrap();
    let dwg = DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
    let m = only_mtext(&dwg);
    assert_eq!(m.attachment_point, AttachmentPoint::MiddleRight);
    assert_eq!(m.column_data.column_type, 2);
}

#[test]
fn embedded_object_group_70_is_used_when_group_71_is_absent() {
    // When group 71 is absent from the entity, embedded object group 70 serves as fallback.
    let dxf = make_mtext_dxf(None, Some(5));
    let doc = read_dxf(dxf);
    let m = only_mtext(&doc);
    assert_eq!(m.attachment_point, AttachmentPoint::MiddleCenter);
    assert_eq!(m.column_data.column_type, 2);
}

#[test]
fn default_top_left_when_both_group_71_and_embedded_group_70_are_absent() {
    let dxf = make_mtext_dxf(None, None);
    let doc = read_dxf(dxf);
    let m = only_mtext(&doc);
    assert_eq!(m.attachment_point, AttachmentPoint::TopLeft);
}
