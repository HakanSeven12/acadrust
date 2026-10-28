use super::import_handles_tests::{fixture, record};
use super::*;
use crate::{DwgReader, DwgWriter, EntityType};

// ACAD 2025 on a Windows-1252 host interpreted these bytes as Windows-1252
// across all eight declarations in Ottosson's checks. Keep that opt-in.
fn encoded_fixture(declared_page: u16) -> Vec<u8> {
    let mut tables: [Vec<EntityRecord>; 10] = std::array::from_fn(|_| Vec::new());
    tables[1] = vec![
        record("LAYER", &[(2, "0")]),
        record("LAYER", &[(2, "P\u{017d}\u{2122}\u{00e4}\u{00f6}")]),
    ];
    tables[2] = vec![record("STYLE", &[(2, "STANDARD"), (3, "txt")])];
    tables[3] = vec![record("LTYPE", &[(2, "CONTINUOUS")])];
    let entities = [
        record(
            "TEXT",
            &[
                (8, "P\u{017d}\u{2122}\u{00e4}\u{00f6}"),
                (1, "\u{017d}\u{2122}\u{201e}\u{00e4}\u{00f6}"),
                (40, "2"),
            ],
        ),
        record(
            "ATTDEF",
            &[
                (8, "0"),
                (1, "\u{017d}\u{2122}\u{201e}\u{00e4}\u{00f6}"),
                (2, "P\u{017d}\u{2122}\u{00e4}\u{00f6}"),
                (3, "\u{017d}\u{2122}\u{201e}\u{00e4}\u{00f6}"),
                (40, "2"),
            ],
        ),
    ];
    let mut bytes = fixture(0x100, &tables, &entities, &[]);
    let mut header = header::read_header(&bytes).unwrap();
    // Exercise strings both before and after the stored code-page field.
    header.variables.menu_name = "\u{017d}\u{2122}\u{201e}\u{00e4}\u{00f6}".into();
    header.variables.dim_arrow_block1 = "P\u{017d}\u{2122}\u{00e4}\u{00f6}".into();
    header.variables.legacy.as_mut().unwrap().user_unit_names[0] =
        "\u{017d}\u{2122}\u{201e}\u{00e4}\u{00f6}".into();
    let encoded = write_header(
        &header.variables,
        header.layout,
        &header.tables,
        header.references,
        0,
        Some(&header.raw),
    )
    .unwrap();
    bytes[..encoded.len()].copy_from_slice(&encoded);
    assert!(bytes
        .windows(5)
        .any(|w| w == [0x8e, 0x99, 0x84, 0xe4, 0xf6]));
    bytes[0x3f9..0x3fb].copy_from_slice(&declared_page.to_le_bytes());
    let crc = crate::io::dwg::crc::crc16(
        crate::io::dwg::crc::CRC16_SEED,
        &bytes[..header::HEADER_VARIABLES_END],
    );
    bytes[header::HEADER_VARIABLES_END..header::HEADER_VARIABLES_END + 2]
        .copy_from_slice(&crc.to_le_bytes());
    bytes
}

fn assert_strings(doc: &CadDocument, text: &str, name: &str) {
    assert_eq!(doc.header.menu_name, text);
    assert_eq!(doc.header.dim_arrow_block1, name);
    assert_eq!(doc.header.legacy.as_ref().unwrap().user_unit_names[0], text);
    assert!(doc.layers.get(name).is_some());
    let texts: Vec<_> = doc
        .entities()
        .filter_map(|e| match e {
            EntityType::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0].value, text);
    assert_eq!(texts[0].common.layer, name);
    let definitions: Vec<_> = doc
        .entities()
        .filter_map(|e| match e {
            EntityType::AttributeDefinition(a) => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].tag, name);
    assert_eq!(definitions[0].default_value, text);
    assert_eq!(definitions[0].prompt, text);
}

#[test]
fn default_r12_decoding_honors_the_declared_code_page() {
    let bytes = encoded_fixture(12);
    let header = header::read_header(&bytes).unwrap();
    assert_eq!(
        header.variables.menu_name,
        "\u{00c4}\u{00d6}\u{00e4}\u{00f5}\u{00f7}"
    );
    let doc = DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
    assert_eq!(doc.header.code_page, "DOS850");
    assert_strings(
        &doc,
        "\u{00c4}\u{00d6}\u{00e4}\u{00f5}\u{00f7}",
        "P\u{00c4}\u{00d6}\u{00f5}\u{00f7}",
    );
}

#[test]
fn explicit_r12_code_page_matches_importer_without_replacing_metadata() {
    for page in [0, 11, 12, 20, 28, 29, 30, 32] {
        let doc = DwgReader::from_stream(Cursor::new(encoded_fixture(page)))
            .with_r12_code_page("ansi_1252")
            .unwrap()
            .read()
            .unwrap();
        assert_eq!(
            doc.header.code_page,
            crate::io::dxf::code_page::dwg_code_page_name(page)
        );
        assert_strings(
            &doc,
            "\u{017d}\u{2122}\u{201e}\u{00e4}\u{00f6}",
            "P\u{017d}\u{2122}\u{00e4}\u{00f6}",
        );
    }
}

#[test]
fn unsupported_r12_code_page_override_is_rejected() {
    for name in ["not-a-code-page", "ASCII", "UTF-8", "utf8", "unicode", ""] {
        assert!(DwgReader::from_stream(Cursor::new(Vec::<u8>::new()))
            .with_r12_code_page(name)
            .is_err());
    }
}

#[test]
fn r12_override_does_not_change_modern_dwg_decoding() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1015);
    doc.header.code_page = "ANSI_1252".into();
    doc.add_entity(EntityType::Text(crate::Text::with_value(
        "\u{00e4}\u{00f6}",
        crate::Vector3::ZERO,
    )))
    .unwrap();
    let bytes = DwgWriter::write_to_vec(&doc).unwrap();
    let restored = DwgReader::from_stream(Cursor::new(bytes))
        .with_r12_code_page("DOS850")
        .unwrap()
        .read()
        .unwrap();
    let text = restored
        .entities()
        .find_map(|e| match e {
            EntityType::Text(t) => Some(&t.value),
            _ => None,
        })
        .unwrap();
    assert_eq!(text, "\u{00e4}\u{00f6}");
}

#[test]
fn resaving_after_override_uses_declared_page_and_preserves_decoded_strings() {
    let mut doc = DwgReader::from_stream(Cursor::new(encoded_fixture(12)))
        .with_r12_code_page("ANSI_1252")
        .unwrap()
        .read()
        .unwrap();
    for _ in 0..3 {
        let saved = DwgWriter::write_to_vec(&doc).unwrap();
        assert_eq!(
            u16::from_le_bytes(saved[0x3f9..0x3fb].try_into().unwrap()),
            12
        );
        assert_eq!(saved, DwgWriter::write_to_vec(&doc).unwrap());
        doc = DwgReader::from_stream(Cursor::new(saved)).read().unwrap();
        assert_eq!(doc.header.code_page, "DOS850");
        assert_strings(
            &doc,
            "\u{017d}\u{2122}\u{201e}\u{00e4}\u{00f6}",
            "P\u{017d}\u{2122}\u{00e4}\u{00f6}",
        );
    }
}

#[test]
fn resaving_after_override_rejects_fixed_header_string_overflow() {
    let mut doc = DwgReader::from_stream(Cursor::new(encoded_fixture(12)))
        .with_r12_code_page("ANSI_1252")
        .unwrap()
        .read()
        .unwrap();
    for value in ["A".repeat(32), "\u{017d}".repeat(4)] {
        doc.header.dim_arrow_block1 = value.clone();
        let saved = DwgWriter::write_to_vec(&doc).unwrap();
        let restored = DwgReader::from_stream(Cursor::new(saved)).read().unwrap();
        assert_eq!(restored.header.dim_arrow_block1, value);
    }
    for value in ["A".repeat(33), "\u{017d}".repeat(5), "A\0B".into()] {
        doc.header.dim_arrow_block1 = value;
        let error = DwgWriter::write_to_vec(&doc).unwrap_err().to_string();
        assert!(error.contains("AC1009 header string"), "{error}");
    }
}
