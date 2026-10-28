use std::collections::HashMap;
use std::io::Cursor;

use acadrust::entities::{EntityType, Line, LwPolyline};
use acadrust::io::dwg::crc::{crc16, CRC16_SEED};
use acadrust::io::dwg::dwg_reader::DwgReadOptions;
use acadrust::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
use acadrust::io::dwg::dwg_stream_readers::handle_reader::read_handles;
use acadrust::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use acadrust::io::dwg::dwg_stream_readers::object_reader::entities::{read_lwpolyline, read_mline};
use acadrust::io::dwg::dwg_stream_readers::object_reader::DwgObjectReader;
use acadrust::io::dwg::dwg_stream_writers::bit_writer::DwgBitWriter;
use acadrust::io::dwg::dwg_stream_writers::object_writer::common::OBJ_LINE;
use acadrust::io::dwg::dwg_version::DwgVersion;
use acadrust::types::{Handle, Vector2, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfVersion};

fn merged_main(writer: DwgBitWriter, version: DxfVersion) -> DwgMergedReader {
    let bit_length = writer.position_in_bits();
    let main = DwgBitReader::new(
        writer.into_bytes(),
        DwgVersion::from_dxf_version(version).unwrap(),
        version,
    );
    let mut reader = DwgMergedReader::from_readers(main, None, None, version);
    reader.set_main_data_end(bit_length);
    reader
}

#[test]
fn impossible_byte_requests_and_truncated_primitives_report_errors() {
    let mut reader = DwgBitReader::new(vec![0x12], DwgVersion::AC24, DxfVersion::AC1024);
    let bytes = reader.read_bytes(usize::MAX);
    assert!(bytes.len() <= 1);
    assert!(reader.check_error().is_err());

    // Seeking back must not erase a decoding failure.
    reader.set_position(0);
    assert!(reader.check_error().is_err());

    let mut reader = DwgBitReader::new(vec![0x12], DwgVersion::AC24, DxfVersion::AC1024);
    reader.read_raw_long();
    assert!(reader.check_error().is_err());

    let mut writer = DwgBitWriter::new(DwgVersion::AC24, DxfVersion::AC1024);
    writer.write_bit(true);
    let mut reader = merged_main(writer, DxfVersion::AC1024);
    assert!(reader.read_bit());
    assert!(reader.check_error().is_ok());
    reader.read_bit();
    assert!(
        reader.check_error().is_err(),
        "byte padding is outside the stream"
    );
}

#[test]
fn impossible_lwpolyline_counts_are_rejected_without_fabricated_vertices() {
    for count in [-1, 100_001, i32::MAX] {
        let mut writer = DwgBitWriter::new(DwgVersion::AC24, DxfVersion::AC1024);
        writer.write_bit_short(0); // No optional LWPOLYLINE arrays.
        writer.write_bit_long(count);
        let mut reader = merged_main(writer, DxfVersion::AC1024);
        let polyline = read_lwpolyline(&mut reader, DwgVersion::AC24);
        assert!(reader.check_error().is_err(), "count {count}");
        assert!(polyline.vertices.is_empty(), "count {count}");
    }
}

#[test]
fn stream_errors_exhaust_remaining_bits_and_modular_overflows_are_rejected() {
    let version = DxfVersion::AC1024;
    let main = DwgBitReader::new(vec![0; 8], DwgVersion::AC24, version);
    let handles = DwgBitReader::new(vec![0x0f, 0xff], DwgVersion::AC24, version);
    let mut reader = DwgMergedReader::from_readers(main, None, Some(handles), version);
    let mut attempts = 0;
    while reader.handle_remaining_bits() >= 8 {
        reader.read_handle();
        attempts += 1;
        assert!(
            attempts <= 2,
            "failed stream must not allow endless handle reads"
        );
    }
    assert!(reader.check_error().is_err());
    assert_eq!(reader.main_remaining_bits(), 0);

    for kind in 0..3 {
        let mut reader = DwgBitReader::new(vec![0xff; 24], DwgVersion::AC24, version);
        match kind {
            0 => {
                reader.read_modular_char();
            }
            1 => {
                reader.read_signed_modular_char();
            }
            _ => {
                reader.read_modular_short();
            }
        }
        assert!(reader.check_error().is_err());
    }
    let objects = DwgObjectReader::new(vec![0xff; 24], version, HashMap::new()).unwrap();
    assert!(objects.read_record_at(0).is_err());
    assert!(objects.type_code_at(0).is_err());
}

#[test]
fn text_lengths_cannot_consume_stream_framing() {
    // A text stream containing only a length claiming five UTF-16 characters,
    // followed by its 16-bit size, flag, and unrelated handle bytes.
    let version = DxfVersion::AC1024;
    let mut writer = DwgBitWriter::new(DwgVersion::AC24, version);
    writer.write_bit_short(5);
    let text_bits = writer.position_in_bits();
    writer.write_raw_short(text_bits as i16);
    let flag = writer.position_in_bits();
    writer.write_bit(true);
    for _ in 0..12 {
        writer.write_byte(0);
    }
    let mut reader = DwgBitReader::new(writer.into_bytes(), DwgVersion::AC24, version);
    assert_eq!(reader.set_position_by_flag(flag), 0);
    assert!(reader.read_variable_text().is_empty());
    assert!(reader.check_error().is_err());
}

#[test]
fn datatable_dimensions_are_bounded_even_when_one_dimension_is_zero() {
    use acadrust::io::dwg::dwg_stream_readers::object_reader::class_object::read_class_object_data;
    for (columns, rows) in [(i32::MAX, 0), (0, i32::MAX), (10_000, 10_000)] {
        let mut writer = DwgBitWriter::new(DwgVersion::AC24, DxfVersion::AC1024);
        writer.write_bit_short(0);
        writer.write_bit_long(columns);
        writer.write_bit_long(rows);
        let mut reader = merged_main(writer, DxfVersion::AC1024);
        read_class_object_data(
            &mut reader,
            "DATATABLE",
            DwgVersion::AC24,
            DxfVersion::AC1024,
        );
        assert!(reader.check_error().is_err());
    }
}

#[test]
fn truncated_handles_and_text_report_errors_without_indexing_missing_bytes() {
    let mut reader = DwgBitReader::new(vec![0x08], DwgVersion::AC24, DxfVersion::AC1024);
    reader.read_handle();
    assert!(reader.check_error().is_err());

    let mut writer = DwgBitWriter::new(DwgVersion::AC24, DxfVersion::AC1024);
    writer.write_bit_short(i16::MAX);
    let mut reader = DwgBitReader::new(writer.into_bytes(), DwgVersion::AC24, DxfVersion::AC1024);
    assert!(reader.read_variable_text().is_empty());
    assert!(reader.check_error().is_err());
}

#[test]
fn signed_mline_counts_cannot_become_usize_capacities() {
    // Exercise the vertex count and both nested parameter counts independently.
    for bad_count in 0..3 {
        let mut writer = DwgBitWriter::new(DwgVersion::AC24, DxfVersion::AC1024);
        writer.write_bit_double(1.0);
        writer.write_byte(0);
        writer.write_3bit_double(Vector3::ZERO);
        writer.write_3bit_double(Vector3::UNIT_Z);
        writer.write_bit_short(0);
        writer.write_byte(1); // One line in the style.
        writer.write_bit_short(if bad_count == 0 { -1 } else { 1 });
        if bad_count != 0 {
            writer.write_3bit_double(Vector3::ZERO);
            writer.write_3bit_double(Vector3::UNIT_X);
            writer.write_3bit_double(Vector3::UNIT_Y);
            writer.write_bit_short(if bad_count == 1 { -1 } else { 0 });
            writer.write_bit_short(if bad_count == 2 { -1 } else { 0 });
        }
        let mut reader = merged_main(writer, DxfVersion::AC1024);
        read_mline(&mut reader);
        assert!(reader.check_error().is_err(), "count field {bad_count}");
    }
}

#[test]
fn graphic_lengths_are_checked_before_allocation_in_both_encodings() {
    for version in [DxfVersion::AC1015, DxfVersion::AC1024] {
        let dwg_version = DwgVersion::from_dxf_version(version).unwrap();
        let mut writer = DwgBitWriter::new(dwg_version, version);
        if version == DxfVersion::AC1015 {
            writer.write_raw_long(0); // Deferred common-data size field.
        }
        writer.write_handle_undefined(1);
        writer.write_bit_short(0); // No extended data.
        writer.write_bit(true); // Graphic data follows.
        if version == DxfVersion::AC1015 {
            writer.write_raw_long(i32::MAX);
        } else {
            writer.write_bit_long_long(13_264_658_366_476);
        }
        let mut reader = merged_main(writer, version);
        let objects = DwgObjectReader::new(Vec::new(), version, HashMap::new()).unwrap();
        let common = objects.read_common_entity_data(&mut reader, OBJ_LINE);
        assert!(reader.check_error().is_err(), "{version:?}");
        assert!(common
            .graphic_data
            .as_deref()
            .unwrap_or_default()
            .is_empty());
    }
}

#[test]
fn negative_graphic_lengths_are_rejected_and_lengths_that_fit_are_read() {
    let payload = [7u8; 32];
    let cases = [
        (DxfVersion::AC1015, -1, false),
        (DxfVersion::AC1015, 32, true),
        (DxfVersion::AC1024, 32, true),
    ];
    for (version, length, fits) in cases {
        let dwg_version = DwgVersion::from_dxf_version(version).unwrap();
        let mut writer = DwgBitWriter::new(dwg_version, version);
        if version == DxfVersion::AC1015 {
            writer.write_raw_long(0); // Deferred common-data size field.
        }
        writer.write_handle_undefined(1);
        writer.write_bit_short(0); // No extended data.
        writer.write_bit(true); // Graphic data follows.
        if version == DxfVersion::AC1015 {
            writer.write_raw_long(length as i32);
        } else {
            writer.write_bit_long_long(length);
        }
        writer.write_bytes(&payload);
        let mut reader = merged_main(writer, version);
        let objects = DwgObjectReader::new(Vec::new(), version, HashMap::new()).unwrap();
        let common = objects.read_common_entity_data(&mut reader, OBJ_LINE);
        let graphic = common.graphic_data.unwrap_or_default();
        if fits {
            // The entity fields after the graphic data are missing, so the record still
            // reports a truncation; only the graphic bytes matter here.
            assert_eq!(graphic, payload, "{version:?}");
        } else {
            assert!(graphic.is_empty(), "{version:?}");
            let error = reader.check_error().unwrap_err().to_string();
            assert!(error.contains("Invalid DWG array count -1"), "{error}");
        }
    }
}

#[test]
fn valid_lwpolyline_over_the_old_count_cap_roundtrips_without_truncation() {
    const COUNT: usize = 100_001;
    for version in [DxfVersion::AC1015, DxfVersion::AC1024] {
        let mut document = CadDocument::with_version(version);
        let point = Vector2::new(12.5, -4.25);
        let handle = document
            .add_entity(EntityType::LwPolyline(LwPolyline::from_points(vec![
                point;
                COUNT
            ])))
            .unwrap();
        let bytes = DwgWriter::write_to_vec(&document).unwrap();
        let read = DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
        let EntityType::LwPolyline(polyline) = read.get_entity(handle).unwrap() else {
            panic!("expected LWPOLYLINE");
        };
        assert_eq!(polyline.vertices.len(), COUNT, "{version:?}");
        assert!(polyline
            .vertices
            .iter()
            .all(|vertex| vertex.location == point));
    }
}

/// Change a generated record's count in place. Locate the record through its
/// handle map and parse its preamble, so no file offsets depend on writer output.
fn dwg_with_impossible_vertex_count() -> (Vec<u8>, Handle, Handle) {
    let version = DxfVersion::AC1015;
    let mut document = CadDocument::with_version(version);
    let healthy = document
        .add_entity(EntityType::Line(Line::from_coords(
            0.0, 0.0, 0.0, 1.0, 2.0, 0.0,
        )))
        .unwrap();
    let malformed = document
        .add_entity(EntityType::LwPolyline(LwPolyline::from_points(vec![
            Vector2::new(7.0, 9.0);
            300
        ])))
        .unwrap();
    let mut bytes = DwgWriter::write_to_vec(&document).unwrap();
    let mut file_reader = DwgReader::from_stream(Cursor::new(bytes.clone()));
    let header = file_reader.read_file_header().unwrap();
    let handles = read_handles(
        &file_reader
            .get_section_buffer("AcDb:Handles", &header)
            .unwrap(),
    )
    .unwrap();
    // In AC1015, the handle map contains absolute file offsets.
    let record_offset = handles[&malformed.value()] as usize;
    let objects = DwgObjectReader::new(bytes.clone(), version, handles).unwrap();
    let (type_code, mut reader) = objects.read_record_at(record_offset).unwrap();
    objects.read_common_entity_data(&mut reader, type_code);
    assert_eq!(reader.read_bit_short() & 0x7f, 0); // No optional fields.
    let count_start = reader.position_in_bits();
    assert_eq!(reader.read_bit_long(), 300);
    let count_bits = reader.position_in_bits() - count_start;
    assert!(reader.check_error().is_ok());

    let mut frame = DwgBitReader::new(bytes[record_offset..].to_vec(), DwgVersion::AC15, version);
    let record_size = frame.read_modular_short() as usize;
    let payload_offset = record_offset + frame.position();
    let mut replacement = DwgBitWriter::new(DwgVersion::AC15, version);
    replacement.write_bit_long(i32::MAX);
    assert_eq!(replacement.position_in_bits(), count_bits);
    let replacement = replacement.into_bytes();
    let bit_start = payload_offset * 8 + count_start as usize;
    for bit in 0..count_bits as usize {
        let output_bit = bit_start + bit;
        let mask = 0x80 >> (output_bit % 8);
        if replacement[bit / 8] & (0x80 >> (bit % 8)) == 0 {
            bytes[output_bit / 8] &= !mask;
        } else {
            bytes[output_bit / 8] |= mask;
        }
    }
    // Keep the checksum valid: this exercises semantic bounds, not CRC rejection.
    let crc_offset = payload_offset + record_size;
    let checksum = crc16(CRC16_SEED, &bytes[record_offset..crc_offset]);
    bytes[crc_offset..crc_offset + 2].copy_from_slice(&checksum.to_le_bytes());
    (bytes, healthy, malformed)
}

#[test]
fn public_reader_rejects_impossible_counts_and_failsafe_skips_the_bad_entity() {
    let (bytes, healthy, malformed) = dwg_with_impossible_vertex_count();
    assert!(DwgReader::from_stream(Cursor::new(bytes.clone()))
        .read()
        .is_err());
    let outcome =
        DwgReader::from_stream_with_options(Cursor::new(bytes), DwgReadOptions::failsafe())
            .read_with_stats()
            .unwrap();
    assert!(outcome.document.get_entity(healthy).is_some());
    assert!(outcome.document.get_entity(malformed).is_none());
    assert!(outcome.stats.skipped_source_records > 0);
    assert!(outcome
        .stats
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.record_handle == Some(malformed.value())));
}
