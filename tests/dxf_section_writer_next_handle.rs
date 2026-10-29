//! `SectionWriter::next_handle` and `set_next_handle` let several writers
//! number the records they create without reusing a handle.
use acadrust::entities::{EntityType, Polyline3D};
use acadrust::io::dxf::{DxfTextWriter, SectionWriter};
use acadrust::types::Vector3;
use acadrust::CadDocument;

fn polyline_drawing() -> CadDocument {
    let mut polyline = Polyline3D::new();
    for x in 0..4 {
        polyline.add_vertex(Vector3::new(f64::from(x), 0.0, 0.0));
    }
    let mut document = CadDocument::new();
    document
        .add_entity(EntityType::Polyline3D(polyline))
        .unwrap();
    document
}

/// The handles (group 5) in DXF text.
fn handles(dxf: &[u8]) -> Vec<u64> {
    let text = String::from_utf8_lossy(dxf);
    let lines: Vec<&str> = text.lines().collect();
    lines
        .chunks(2)
        .filter(|pair| pair.len() == 2 && pair[0].trim() == "5")
        .map(|pair| u64::from_str_radix(pair[1].trim(), 16).unwrap())
        .collect()
}

/// Write `document`'s entities with a writer numbering from `start`, and
/// return the output and the writer's next handle.
fn write_entities(document: &CadDocument, start: Option<u64>) -> (Vec<u8>, u64) {
    let mut dxf = Vec::new();
    let next = {
        let mut text = DxfTextWriter::new(&mut dxf);
        let mut writer = SectionWriter::new(&mut text, 0x1000, 0x2000);
        assert_eq!(writer.next_handle(), 0x1000);
        if let Some(start) = start {
            writer.set_next_handle(start);
            assert_eq!(writer.next_handle(), start);
        }
        writer.write_entities(document).unwrap();
        writer.next_handle()
    };
    (dxf, next)
}

#[test]
fn next_handle_counts_the_records_the_writer_creates() {
    let (_, next) = write_entities(&polyline_drawing(), None);
    // Four VERTEX records and a SEQEND.
    assert_eq!(next, 0x1005);
}

#[test]
fn a_writer_continues_from_set_next_handle() {
    let document = polyline_drawing();
    let (first, next) = write_entities(&document, None);
    let (second, after) = write_entities(&document, Some(next));
    assert_eq!(after, next + 5);

    let created = |dxf: &[u8]| -> Vec<u64> {
        handles(dxf)
            .into_iter()
            .filter(|&handle| handle >= 0x1000)
            .collect()
    };
    assert_eq!(created(&first), (0x1000..0x1005).collect::<Vec<_>>());
    assert_eq!(created(&second), (next..next + 5).collect::<Vec<_>>());
}
