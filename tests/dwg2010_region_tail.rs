use acadrust::entities::acis::primitives::build_box;
use acadrust::entities::solid3d::AcisMaterial;
use acadrust::io::dwg::dwg_reference_type::DwgReferenceType;
use acadrust::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
use acadrust::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use acadrust::io::dwg::dwg_stream_readers::object_reader::entities::read_acis_entity;
use acadrust::io::dwg::dwg_stream_writers::bit_writer::DwgBitWriter;
use acadrust::io::dwg::DwgVersion;
use acadrust::{DxfVersion, Handle, SabWriter};

// ODA DWG section 20.4.41 keeps the silhouette count when the isoline cache
// is absent, including BL(0). A stream that skips the count is incomplete.
fn record(
    offset: i64,
    silhouettes: Option<i32>,
    materials: i32,
) -> (DwgMergedReader, Vec<u8>, i64) {
    let version = DwgVersion::AC24;
    let dxf = DxfVersion::AC1024;
    let sab = SabWriter::write(&build_box([1.0, 2.0, 3.0], 2.0, 3.0, 4.0));
    let mut main = DwgBitWriter::new(version, dxf);
    let mut handles = DwgBitWriter::new(version, dxf);
    for _ in 0..offset {
        main.write_bit(true);
    }
    main.write_bit(false); // nonempty inline SAB
    main.write_bit(false);
    main.write_bit_short(2);
    main.write_bytes(&sab);
    main.write_bit(true); // wireframe header
    main.write_bit(false); // no reference point
    main.write_bit_long(0); // isolines
    main.write_bit(false); // no isoline cache
    if let Some(count) = silhouettes {
        main.write_bit_long(count);
    }
    main.write_bit(true); // no extra modeler body
    main.write_bit_long(materials);
    for _ in 0..materials.max(0) {
        main.write_bit_long(7);
        main.write_bit_long(9);
        handles.write_handle(DwgReferenceType::HardPointer, 0x1234);
    }
    let end = main.position_in_bits();
    let mut reader = DwgMergedReader::from_readers(
        DwgBitReader::new(main.into_bytes(), version, dxf),
        None,
        Some(DwgBitReader::new(handles.into_bytes(), version, dxf)),
        dxf,
    );
    reader.set_main_data_end(end);
    reader.set_position_in_bits(offset);
    (reader, sab, end)
}

#[test]
fn ac1024_empty_silhouette_count_preserves_geometry_and_materials() {
    for offset in 0..8 {
        for materials in [0, 1] {
            let (mut reader, sab, end) = record(offset, Some(0), materials);
            let data = read_acis_entity(&mut reader, DwgVersion::AC24, DxfVersion::AC1024, false);
            reader
                .check_error()
                .unwrap_or_else(|error| panic!("offset={offset}, materials={materials}: {error}"));
            assert_eq!(reader.position_in_bits(), end);
            assert_eq!(data.sab_data, sab);
            assert_eq!(data.version, 2);
            assert!(data.is_binary);
            assert!(data.wireframe_data_present);
            assert!(!data.wireframe_point_present);
            assert!(!data.wireframe_isoline_present);
            assert_eq!(data.isolines, 0);
            assert!(data.wires.is_empty());
            assert!(data.silhouettes.is_empty());
            assert!(data.acis_empty_bit);
            assert!(data.extra_acis_data.is_none());
            let expected = if materials == 0 {
                vec![]
            } else {
                vec![AcisMaterial {
                    array_index: 7,
                    absolute_reference: 9,
                    material_handle: Some(Handle::new(0x1234)),
                }]
            };
            assert_eq!(data.materials, expected);

            let (mut reader, _, end) = record(offset, None, materials);
            read_acis_entity(&mut reader, DwgVersion::AC24, DxfVersion::AC1024, false);
            assert!(
                reader.check_error().is_err() || reader.position_in_bits() != end,
                "missing silhouette count accepted: offset={offset}, materials={materials}"
            );
        }
    }
}

#[test]
fn ac1024_silhouette_count_does_not_accept_truncation_or_invalid_counts() {
    for offset in 0..8 {
        for silhouettes in [None, Some(0)] {
            for materials in [0, 1] {
                let (mut reader, _, end) = record(offset, silhouettes, materials);
                reader.set_main_data_end(end - 1);
                read_acis_entity(&mut reader, DwgVersion::AC24, DxfVersion::AC1024, false);
                assert!(
                    reader.check_error().is_err() || reader.position_in_bits() != end - 1,
                    "truncated tail accepted completely: offset={offset}, silhouettes={silhouettes:?}, materials={materials}"
                );
            }
            let (mut reader, _, end) = record(offset, silhouettes, -1);
            read_acis_entity(&mut reader, DwgVersion::AC24, DxfVersion::AC1024, false);
            assert!(
                reader.check_error().is_err() || reader.position_in_bits() != end,
                "negative material count accepted completely"
            );
        }
        let (mut reader, _, end) = record(offset, Some(1), 0);
        read_acis_entity(&mut reader, DwgVersion::AC24, DxfVersion::AC1024, false);
        assert!(
            reader.check_error().is_err() || reader.position_in_bits() != end,
            "nonempty absent cache accepted completely"
        );
    }
}

#[test]
fn missing_material_count_is_not_a_complete_record() {
    for offset in 0..8 {
        for silhouettes in [None, Some(0)] {
            let (mut reader, _, end) = record(offset, silhouettes, 0);
            let bound = end - 2; // Remove mandatory BL(0), retaining physical bytes.
            reader.set_main_data_end(bound);
            read_acis_entity(&mut reader, DwgVersion::AC24, DxfVersion::AC1024, false);
            assert!(reader.check_error().is_err() || reader.position_in_bits() != bound);
        }
    }
}

#[test]
fn multiple_material_bindings_are_preserved() {
    for silhouettes in [Some(0)] {
        let (mut reader, sab, end) = record(3, silhouettes, 2);
        let data = read_acis_entity(&mut reader, DwgVersion::AC24, DxfVersion::AC1024, false);
        reader.check_error().unwrap();
        assert_eq!(reader.position_in_bits(), end);
        assert_eq!(data.sab_data, sab);
        assert_eq!(data.materials.len(), 2);
        assert!(data.materials.iter().all(|m| m.array_index == 7
            && m.absolute_reference == 9
            && m.material_handle == Some(Handle::new(0x1234))));
    }
}
