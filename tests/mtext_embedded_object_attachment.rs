//! An R2018 MTEXT with columns preserves the attachment point of group 71.
//!
//! In an AC1032 DXF the column layout of an MTEXT lives in a `101 Embedded
//! Object` block. Group 71 on the entity specifies the attachment point used
//! by DWG and CAD applications (such as ODA and AutoCAD). Group 70 in the
//! embedded object serves as a fallback when group 71 is absent (#249).

use std::io::Cursor;

use acadrust::entities::{AttachmentPoint, EntityType, MText};
use acadrust::types::Vector3;
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfVersion, DxfWriter};

/// An AC1032 DXF whose only MTEXT has dynamic columns, group 71 = 5 and an
/// embedded object with group 70 = `embedded_attachment`.
fn dxf(embedded_attachment: Option<i16>) -> Vec<u8> {
    let mut m = MText::with_value("{\\fArial|b0|i1|c0|p34;01}", Vector3::new(50.0, 50.0, 0.0));
    m.height = 2.5;
    m.rectangle_width = 38.8;
    m.attachment_point = AttachmentPoint::MiddleCenter;
    if embedded_attachment.is_some() {
        m.column_data.column_type = 2;
        m.column_data.column_count = 1;
        m.column_data.width = 38.8;
        m.column_data.gutter = 12.5;
        m.column_data.heights = vec![-2.552864938608458];
    }
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    doc.add_entity(EntityType::MText(Box::new(m))).unwrap();
    let text = String::from_utf8(DxfWriter::new(&doc).write_to_vec().unwrap()).unwrap();

    let Some(value) = embedded_attachment else {
        assert!(!text.contains("Embedded Object"));
        return text.into_bytes();
    };
    // Rewrite the embedded object's group 70 (the pair right after the marker).
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let marker = lines
        .iter()
        .position(|l| l.trim() == "Embedded Object")
        .expect("the column MTEXT has an embedded object");
    assert_eq!(lines[marker + 1].trim(), "70");
    lines[marker + 2] = format!("{value:6}");
    lines.join("\r\n").into_bytes()
}

/// An AC1032 DXF where group 71 is omitted, leaving only the embedded object's group 70.
fn dxf_without_group_71(embedded_attachment: i16) -> Vec<u8> {
    let bytes = dxf(Some(embedded_attachment));
    let text = String::from_utf8(bytes).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let marker = lines
        .iter()
        .position(|l| l.trim() == "Embedded Object")
        .expect("embedded object");
    if let Some(pos) = lines[..marker].iter().rposition(|l| l.trim() == "71") {
        lines.remove(pos + 1);
        lines.remove(pos);
    }
    lines.join("\r\n").into_bytes()
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
fn group_71_attachment_wins_over_embedded_object_group_70() {
    let m = only_mtext(&read_dxf(dxf(Some(1))));
    assert_eq!(m.attachment_point, AttachmentPoint::MiddleCenter);
    assert_eq!(m.column_data.column_type, 2, "the columns are still read");
    assert_eq!(m.column_data.heights, vec![-2.552864938608458]);
}

#[test]
fn group_71_attachment_reaches_the_dwg() {
    let doc = read_dxf(dxf(Some(1)));
    let bytes = DwgWriter::write_to_vec(&doc).unwrap();
    let dwg = DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
    let m = only_mtext(&dwg);
    assert_eq!(m.attachment_point, AttachmentPoint::MiddleCenter);
    assert_eq!(m.column_data.column_type, 2);
}

#[test]
fn group_71_attachment_survives_a_dxf_round_trip() {
    let doc = read_dxf(dxf(Some(1)));
    let bytes = DxfWriter::new(&doc).write_to_vec().unwrap();
    let m = only_mtext(&read_dxf(bytes));
    assert_eq!(m.attachment_point, AttachmentPoint::MiddleCenter);
}

#[test]
fn embedded_object_attachment_used_when_group_71_is_absent() {
    let m = only_mtext(&read_dxf(dxf_without_group_71(1)));
    assert_eq!(m.attachment_point, AttachmentPoint::TopLeft);
    assert_eq!(m.column_data.column_type, 2);
}

#[test]
fn without_an_embedded_object_group_71_is_the_attachment() {
    let m = only_mtext(&read_dxf(dxf(None)));
    assert_eq!(m.attachment_point, AttachmentPoint::MiddleCenter);
    assert_eq!(m.column_data.column_type, 0);
}
