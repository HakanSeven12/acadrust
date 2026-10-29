use acadrust::{
    objects::{
        BlockPropertiesTable, BlockPropertiesTableField, BlockPropertiesTableValue,
        DynamicBlockData, ObjectType,
    },
    types::Handle,
    CadDocument, DwgReader, DwgWriter, DxfReader, DxfWriter,
};
use std::io::Cursor;

fn read(bytes: &[u8]) -> CadDocument {
    DwgReader::from_stream(Cursor::new(bytes)).read().unwrap()
}

fn data(doc: &CadDocument, handle: u64) -> &DynamicBlockData {
    match &doc.objects[&Handle::new(handle)] {
        ObjectType::DynamicBlock(object) => &object.data,
        object => panic!("Expected a decoded dynamic object: {object:?}"),
    }
}

#[test]
fn native_action_fields_survive_dwg_and_dxf() {
    let cases: &[(&[u8], &[u64])] = &[
        (
            include_bytes!("fixtures/dynamic-blocks/point.dwg"),
            &[0x267],
        ),
        (
            include_bytes!("fixtures/dynamic-blocks/xy.dwg"),
            &[0x282, 0x28F],
        ),
        (
            include_bytes!("fixtures/dynamic-blocks/lookup.dwg"),
            &[0x603, 0x607],
        ),
        (
            include_bytes!("fixtures/dynamic-blocks/polar.dwg"),
            &[0x3A5, 0x3AC],
        ),
    ];
    for (bytes, handles) in cases {
        let doc = read(bytes);
        let saved = read(&DwgWriter::write_to_vec(&doc).unwrap());
        let dxf = DxfWriter::new(&doc).write_to_vec().unwrap();
        let dxf = DxfReader::from_reader(Cursor::new(dxf))
            .unwrap()
            .read()
            .unwrap();
        for h in *handles {
            let original = data(&doc, *h);
            assert_eq!(original, data(&saved, *h), "DWG object {h:X}");
            match original {
                DynamicBlockData::MoveAction(value) => {
                    assert_eq!(value.offsets.distance_multiplier, 1.0);
                    assert_eq!(value.offsets.angle_offset, 0.0);
                    let DynamicBlockData::MoveAction(other) = data(&dxf, *h) else {
                        panic!()
                    };
                    assert_eq!(value.offsets, other.offsets);
                }
                DynamicBlockData::ArrayAction(value) => {
                    assert_eq!(value.row_offset, 20.0);
                    assert_eq!(value.column_offset, 25.0);
                    let DynamicBlockData::ArrayAction(other) = data(&dxf, *h) else {
                        panic!()
                    };
                    assert_eq!((other.row_offset, other.column_offset), (20.0, 25.0));
                }
                DynamicBlockData::LookupAction(value) => {
                    assert_eq!(value.columns.len(), value.column_count as usize);
                    assert_eq!(
                        value.expressions.len(),
                        (value.row_count * value.column_count) as usize
                    );
                    assert!(value
                        .columns
                        .iter()
                        .any(|c| c.lookup_property && c.writable));
                    let DynamicBlockData::LookupAction(other) = data(&dxf, *h) else {
                        panic!()
                    };
                    assert_eq!(value.columns, other.columns);
                    assert_eq!(value.expressions, other.expressions);
                }
                DynamicBlockData::PolarStretchAction(value) => {
                    assert!(!value.bindings.is_empty());
                    assert!(value.bindings.iter().any(|b| !b.indexes.is_empty()));
                    assert_eq!(value.distance_multiplier, 1.0);
                    let DynamicBlockData::PolarStretchAction(other) = data(&dxf, *h) else {
                        panic!()
                    };
                    assert_eq!(value.bindings, other.bindings);
                    assert_eq!(value.handles, other.handles);
                    assert_eq!(value.points, other.points);
                    assert_eq!(value.codes, other.codes);
                }
                DynamicBlockData::XYParameter(value) => {
                    assert!(!value.x_label.is_empty());
                    assert_ne!(value.x_label, value.y_label);
                    let DynamicBlockData::XYParameter(other) = data(&dxf, *h) else {
                        panic!()
                    };
                    assert_eq!(
                        (&value.x_label, &value.y_label),
                        (&other.x_label, &other.y_label)
                    );
                }
                DynamicBlockData::PolarParameter(value) => {
                    assert!(!value.distance_name.is_empty());
                    let DynamicBlockData::PolarParameter(other) = data(&dxf, *h) else {
                        panic!()
                    };
                    assert_eq!(
                        (&value.distance_name, &value.angle_name),
                        (&other.distance_name, &other.angle_name)
                    );
                    assert_eq!(value.distance_value_set, other.distance_value_set);
                    assert_eq!(value.angle_value_set, other.angle_value_set);
                }
                DynamicBlockData::LookupParameter(value) => {
                    let DynamicBlockData::LookupParameter(other) = data(&dxf, *h) else {
                        panic!()
                    };
                    assert_eq!(value.lookup_name, other.lookup_name);
                }
                value => panic!("Unexpected fixture object {value:?}"),
            }
        }
    }
}

#[test]
fn properties_table_numeric_rows_roundtrip_in_dxf_and_dwg() {
    let mut doc = read(include_bytes!("fixtures/dynamic-blocks/point.dwg"));
    let handle = Handle::new(0x25C);
    let ObjectType::DynamicBlock(object) = doc.objects.get_mut(&handle).unwrap() else {
        panic!()
    };
    object.dxf_name = "BLOCKPROPERTIESTABLE".into();
    object.cpp_class_name = "AcDbBlockPropertiesTable".into();
    object.data = DynamicBlockData::PropertiesTable(BlockPropertiesTable {
        parameter: Default::default(),
        fields: vec![
            BlockPropertiesTableField {
                code: 90,
                value: BlockPropertiesTableValue::Integer(2),
            },
            BlockPropertiesTableField {
                code: 300,
                value: BlockPropertiesTableValue::Text("Example table".into()),
            },
            BlockPropertiesTableField {
                code: 301,
                value: BlockPropertiesTableValue::Text(String::new()),
            },
            BlockPropertiesTableField {
                code: 91,
                value: BlockPropertiesTableValue::Integer(2),
            },
            BlockPropertiesTableField {
                code: 340,
                value: BlockPropertiesTableValue::Handle(Handle::NULL),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(0),
            },
            BlockPropertiesTableField {
                code: 171,
                value: BlockPropertiesTableValue::Integer(-1),
            },
            BlockPropertiesTableField {
                code: 300,
                value: BlockPropertiesTableValue::Text(String::new()),
            },
            BlockPropertiesTableField {
                code: 301,
                value: BlockPropertiesTableValue::Text("UpdatedDistance".into()),
            },
            BlockPropertiesTableField {
                code: 90,
                value: BlockPropertiesTableValue::Integer(14),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(-9999),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(-9999),
            },
            BlockPropertiesTableField {
                code: 290,
                value: BlockPropertiesTableValue::Boolean(false),
            },
            BlockPropertiesTableField {
                code: 291,
                value: BlockPropertiesTableValue::Boolean(true),
            },
            BlockPropertiesTableField {
                code: 292,
                value: BlockPropertiesTableValue::Boolean(true),
            },
            BlockPropertiesTableField {
                code: 293,
                value: BlockPropertiesTableValue::Boolean(false),
            },
            BlockPropertiesTableField {
                code: 294,
                value: BlockPropertiesTableValue::Boolean(false),
            },
            BlockPropertiesTableField {
                code: 302,
                value: BlockPropertiesTableValue::Text(String::new()),
            },
            BlockPropertiesTableField {
                code: 340,
                value: BlockPropertiesTableValue::Handle(Handle::NULL),
            },
            BlockPropertiesTableField {
                code: 340,
                value: BlockPropertiesTableValue::Handle(Handle::NULL),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(0),
            },
            BlockPropertiesTableField {
                code: 171,
                value: BlockPropertiesTableValue::Integer(-1),
            },
            BlockPropertiesTableField {
                code: 300,
                value: BlockPropertiesTableValue::Text(String::new()),
            },
            BlockPropertiesTableField {
                code: 301,
                value: BlockPropertiesTableValue::Text("UpdatedAngle".into()),
            },
            BlockPropertiesTableField {
                code: 90,
                value: BlockPropertiesTableValue::Integer(21),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(-9999),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(-9999),
            },
            BlockPropertiesTableField {
                code: 290,
                value: BlockPropertiesTableValue::Boolean(false),
            },
            BlockPropertiesTableField {
                code: 291,
                value: BlockPropertiesTableValue::Boolean(true),
            },
            BlockPropertiesTableField {
                code: 292,
                value: BlockPropertiesTableValue::Boolean(true),
            },
            BlockPropertiesTableField {
                code: 293,
                value: BlockPropertiesTableValue::Boolean(true),
            },
            BlockPropertiesTableField {
                code: 294,
                value: BlockPropertiesTableValue::Boolean(false),
            },
            BlockPropertiesTableField {
                code: 302,
                value: BlockPropertiesTableValue::Text(String::new()),
            },
            BlockPropertiesTableField {
                code: 340,
                value: BlockPropertiesTableValue::Handle(Handle::NULL),
            },
            BlockPropertiesTableField {
                code: 92,
                value: BlockPropertiesTableValue::Integer(2),
            },
            BlockPropertiesTableField {
                code: 90,
                value: BlockPropertiesTableValue::Integer(0),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(40),
            },
            BlockPropertiesTableField {
                code: 140,
                value: BlockPropertiesTableValue::Real(2.0),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(40),
            },
            BlockPropertiesTableField {
                code: 140,
                value: BlockPropertiesTableValue::Real(0.75),
            },
            BlockPropertiesTableField {
                code: 90,
                value: BlockPropertiesTableValue::Integer(1),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(40),
            },
            BlockPropertiesTableField {
                code: 140,
                value: BlockPropertiesTableValue::Real(4.0),
            },
            BlockPropertiesTableField {
                code: 170,
                value: BlockPropertiesTableValue::Integer(40),
            },
            BlockPropertiesTableField {
                code: 140,
                value: BlockPropertiesTableValue::Real(1.5),
            },
            BlockPropertiesTableField {
                code: 93,
                value: BlockPropertiesTableValue::Integer(3),
            },
            BlockPropertiesTableField {
                code: 290,
                value: BlockPropertiesTableValue::Boolean(false),
            },
            BlockPropertiesTableField {
                code: 291,
                value: BlockPropertiesTableValue::Boolean(true),
            },
            BlockPropertiesTableField {
                code: 292,
                value: BlockPropertiesTableValue::Boolean(false),
            },
        ],
    });
    let mut class = doc
        .classes
        .get_by_name("BLOCKPOINTPARAMETER")
        .unwrap()
        .clone();
    class.dxf_name = "BLOCKPROPERTIESTABLE".into();
    class.cpp_class_name = "AcDbBlockPropertiesTable".into();
    doc.classes.add_or_update(class);

    let dxf = DxfWriter::new(&doc).write_to_vec().unwrap();
    let decoded = DxfReader::from_reader(Cursor::new(dxf))
        .unwrap()
        .read()
        .unwrap();
    assert_eq!(data(&doc, handle.value()), data(&decoded, handle.value()));

    let dwg = DwgWriter::write_to_vec(&doc).unwrap();
    let decoded = DwgReader::from_stream(Cursor::new(dwg)).read().unwrap();
    assert_eq!(data(&doc, handle.value()), data(&decoded, handle.value()));

    let mut unsupported = doc.clone();
    let ObjectType::DynamicBlock(object) = unsupported.objects.get_mut(&handle).unwrap() else {
        panic!()
    };
    let DynamicBlockData::PropertiesTable(table) = &mut object.data else {
        panic!()
    };
    table.fields.push(BlockPropertiesTableField {
        code: 310,
        value: BlockPropertiesTableValue::Binary(vec![0xAA]),
    });
    let error = DwgWriter::write_to_vec(&unsupported)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unsupported value type"), "{error}");
}
