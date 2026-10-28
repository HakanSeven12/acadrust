use acadrust::{
    objects::{BlockEvalValue, DynamicBlockData, ObjectType},
    CadDocument, DwgReader, EntityType,
};
use std::io::Cursor;

#[path = "support/dynamic_property_fixtures.rs"]
mod fixture_data;

fn read_fixture(name: &str) -> CadDocument {
    DwgReader::from_stream(Cursor::new(fixture_data::fixture(name)))
        .read()
        .unwrap()
}

#[test]
fn saved_parameters_and_definition_defaults_match_autocad() {
    let cases = [
        ("BLOCKLINEARPARAMETER", "my_distance_param", 5.0, "284", 7.5),
        (
            "BLOCKROTATIONPARAMETER",
            "Angle parameter",
            std::f64::consts::FRAC_PI_2,
            "27C",
            0.8726646259971648,
        ),
        ("BLOCKPOINTPARAMETER", "dynamic_pos X", 0.0, "294", 5.0),
        (
            "BLOCKPOLARPARAMETER",
            "Distance1",
            9.081824427499454,
            "299",
            16.854057277965502,
        ),
        (
            "BLOCKXYPARAMETER",
            "X Distance1",
            5.0,
            "2AE",
            100.59269849759175,
        ),
    ];
    for (file, property, expected_default, instance, expected_saved) in cases {
        let bytes = fixture_data::fixture(file);
        let doc = read_fixture(file);
        let handle = acadrust::Handle::new(u64::from_str_radix(instance, 16).unwrap());
        let definition = doc.dynamic_definition_for_insert(handle).unwrap();
        for (insert, expected) in [(None, expected_default), (Some(handle), expected_saved)] {
            let properties = doc.dynamic_block_properties(definition, insert).unwrap();
            let p = properties.iter().find(|p| p.name == property).unwrap();
            let BlockEvalValue::Real(actual) = p.value else {
                panic!("Expected real {p:?}");
            };
            assert!(
                (actual - expected).abs() < 1e-9,
                "{file} {property}: {actual} != {expected}"
            );
        }
        assert_eq!(fixture_data::fixture(file), bytes);
    }
}

#[test]
fn visibility_and_flip_are_typed_choices_with_saved_states() {
    for (file, instance, expected) in [
        (
            "BLOCKVISIBILITYPARAMETER",
            0x28C,
            BlockEvalValue::Text("ShowAll".into()),
        ),
        ("BLOCKFLIPPARAMETER", 0x28B, BlockEvalValue::Short(1)),
    ] {
        let doc = read_fixture(file);
        let handle = acadrust::Handle::new(instance);
        let definition = doc.dynamic_definition_for_insert(handle).unwrap();
        let values = doc
            .dynamic_block_properties(definition, Some(handle))
            .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].value, expected);
        assert!(values[0].allowed_values.contains(&expected));
        assert_ne!(
            doc.dynamic_block_properties(definition, None).unwrap()[0].value,
            expected
        );
    }
}

#[test]
fn lookup_descriptors_are_per_column_and_preserve_choices_and_custom_labels() {
    let doc = read_fixture("BLOCKLOOKUPPARAMETER");
    let action = doc
        .objects
        .values()
        .find_map(|o| match o {
            ObjectType::DynamicBlock(o) => match &o.data {
                DynamicBlockData::LookupAction(a) => Some(a),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (
            action.row_count,
            action.column_count,
            action.expressions.len(),
            action.columns.len()
        ),
        (3, 5, 15, 5)
    );
    assert_eq!(action.columns[3].node_id, 20);
    assert_eq!(action.columns[3].unmatched_name, "my_custom");
    assert_eq!(action.columns[3].connection_name, "lookupString");
    let definition = doc.block_records.get("My_Look_Block").unwrap().handle;
    let properties = doc.dynamic_block_properties(definition, None).unwrap();
    let lookup = properties
        .iter()
        .find(|p| p.name == "Lookup Label Name")
        .unwrap();
    assert!(!lookup.read_only);
    assert_eq!(lookup.value, BlockEvalValue::Text("my_custom".into()));
    assert_eq!(
        lookup.allowed_values,
        ["Size 5", "Size 6 ", "Size 8", "my_custom"].map(|s| BlockEvalValue::Text(s.into()))
    );
    for entity in doc.entities() {
        if let EntityType::Insert(i) = entity {
            assert!(doc
                .dynamic_block_properties(definition, Some(i.common.handle))
                .is_ok());
        }
    }
}

#[test]
fn scale_changes_distances_but_not_definition_defaults() {
    let mut doc = read_fixture("BLOCKLINEARPARAMETER");
    let handle = acadrust::Handle::new(0x284);
    let definition = doc.dynamic_definition_for_insert(handle).unwrap();
    for (x, y, z, expected) in [
        (2., 2., 2., Some(15.)),
        (-2., 2., 2., Some(15.)),
        (2., 3., 4., None),
    ] {
        let EntityType::Insert(i) = doc.get_entity_mut(handle).unwrap() else {
            panic!()
        };
        i.set_x_scale(x);
        i.set_y_scale(y);
        i.set_z_scale(z);
        let values = doc
            .dynamic_block_properties(definition, Some(handle))
            .unwrap();
        if let Some(expected) = expected {
            assert_eq!(
                values
                    .iter()
                    .find(|p| p.name == "my_distance_param")
                    .unwrap()
                    .value,
                BlockEvalValue::Real(expected)
            );
        } else {
            assert!(values.is_empty());
        }
        assert_eq!(
            doc.dynamic_block_properties(definition, None).unwrap()[0].value,
            BlockEvalValue::Real(5.)
        );
    }
}

#[test]
fn older_drawings_restore_dynamic_definition_names() {
    let doc = read_fixture("LOOKUP_R2004");
    let definition = doc.block_records.get("My_Look_Block").unwrap();
    assert!(!definition.flags.anonymous);
    assert!(doc.is_dynamic_block(definition.handle));
    assert_eq!(
        doc.dynamic_block_properties(definition.handle, None)
            .unwrap()
            .len(),
        5
    );
    // Cached representations can carry the same true-name XData; do not create
    // extra named definitions from those copies.
    let polar = read_fixture("BLOCKPOLARPARAMETER");
    assert_eq!(
        polar
            .block_records
            .iter()
            .filter(|b| !b.name.starts_with('*'))
            .count(),
        1
    );
}

#[test]
fn rotation_angles_above_pi_are_not_folded() {
    let doc = read_fixture("ROTATION_270");
    let insert = acadrust::Handle::new(0x27C);
    let definition = doc.dynamic_definition_for_insert(insert).unwrap();
    let properties = doc
        .dynamic_block_properties(definition, Some(insert))
        .unwrap();
    assert_eq!(
        properties[0].value,
        BlockEvalValue::Real(std::f64::consts::PI * 1.5)
    );
}

#[test]
fn stale_marker_suffixes_do_not_override_autocad_reopen_names() {
    let doc = read_fixture("LOOKUP_REOPEN");
    for (handle, expected) in [
        (0x254, "*U2"),
        (0x38C, "*U3"),
        (0x395, "*U4"),
        (0x39E, "*U5"),
    ] {
        let EntityType::Insert(insert) = doc.get_entity(acadrust::Handle::new(handle)).unwrap()
        else {
            panic!()
        };
        assert_eq!(insert.block_name, expected);
        let block = doc.block_records.get(expected).unwrap();
        let Some(EntityType::Block(marker)) = doc.get_entity(block.block_entity_handle) else {
            panic!()
        };
        assert_eq!(marker.name, expected);
    }
}

#[test]
fn reflected_rotation_keeps_its_parameter_orientation_and_rejects_broken_caches() {
    use acadrust::{objects::*, Handle};
    let mut doc = read_fixture("ROTATION_270");
    let insert = Handle::new(0x27C);
    let definition = doc.dynamic_definition_for_insert(insert).unwrap();
    let parameter = doc
        .dynamic_block_properties(definition, Some(insert))
        .unwrap()[0]
        .parameter_handle;
    let node_id = match &doc.objects[&parameter] {
        ObjectType::DynamicBlock(DynamicBlockObject {
            data: DynamicBlockData::RotationParameter(p),
            ..
        }) => p.parameter.parameter.element.eval.node_id,
        _ => panic!(),
    };
    let graph = doc
        .objects
        .iter()
        .find_map(|(h, o)| match o {
            ObjectType::DynamicBlock(DynamicBlockObject {
                data: DynamicBlockData::EvaluationGraph(g),
                ..
            }) if g.nodes.iter().any(|n| n.expression == parameter) => Some(*h),
            _ => None,
        })
        .unwrap();
    let dict = |doc: &acadrust::CadDocument, h, key| match &doc.objects[&h] {
        ObjectType::Dictionary(d) => d.get(key).unwrap(),
        _ => panic!(),
    };
    let mut cache = doc.extension_dictionary_handle(insert).unwrap();
    for key in [
        "AcDbBlockRepresentation",
        "AppDataCache",
        "ACAD_ENHANCEDBLOCKDATA",
    ] {
        cache = dict(&doc, cache, key);
    }
    let rotation_cache = dict(&doc, cache, &node_id.to_string());
    let flip_cache = doc.allocate_handle();
    let mut state = XRecord::named("9876");
    state.handle = flip_cache;
    state.owner = cache;
    state.entries = vec![
        XRecordEntry {
            code: 70,
            value: XRecordValue::Int16(0)
        };
        4
    ];
    state.entries.push(XRecordEntry {
        code: 70,
        value: XRecordValue::Int16(1),
    });
    doc.objects.insert(flip_cache, ObjectType::XRecord(state));
    let ObjectType::Dictionary(d) = doc.objects.get_mut(&cache).unwrap() else {
        panic!()
    };
    d.entries.push(("9876".into(), flip_cache));
    let action_handle = doc.allocate_handle();
    let mut action = BlockFlipAction::default();
    action.action.dependencies.push(parameter);
    action.connections[0].code = 9876;
    doc.objects.insert(
        action_handle,
        ObjectType::DynamicBlock(DynamicBlockObject {
            handle: action_handle,
            data: DynamicBlockData::FlipAction(action),
            ..Default::default()
        }),
    );
    let ObjectType::DynamicBlock(DynamicBlockObject {
        data: DynamicBlockData::EvaluationGraph(g),
        ..
    }) = doc.objects.get_mut(&graph).unwrap()
    else {
        panic!()
    };
    let mut node = g.nodes[0].clone();
    node.expression = action_handle;
    g.nodes.push(node);
    assert_eq!(
        doc.dynamic_block_properties(definition, Some(insert))
            .unwrap()[0]
            .value,
        BlockEvalValue::Real(std::f64::consts::FRAC_PI_2)
    );
    let ObjectType::XRecord(record) = doc.objects.get_mut(&rotation_cache).unwrap() else {
        panic!()
    };
    record.entries.clear();
    assert!(doc
        .dynamic_block_properties(definition, Some(insert))
        .unwrap_err()
        .to_string()
        .contains("Malformed"));
}

#[test]
fn lookup_and_axis_descriptors_survive_dwg_and_dxf_roundtrips() {
    use acadrust::{DwgWriter, DxfReader, DxfWriter};
    use std::io::Cursor;
    for name in [
        "BLOCKLOOKUPPARAMETER",
        "BLOCKPOLARPARAMETER",
        "BLOCKXYPARAMETER",
    ] {
        let doc = read_fixture(name);
        let dwg = DwgReader::from_stream(Cursor::new(DwgWriter::write_to_vec(&doc).unwrap()))
            .read()
            .unwrap();
        let dxf = DxfReader::from_reader(Cursor::new(DxfWriter::new(&doc).write_to_vec().unwrap()))
            .unwrap()
            .read()
            .unwrap();
        for (handle, object) in &doc.objects {
            let ObjectType::DynamicBlock(object) = object else {
                continue;
            };
            if !matches!(
                object.data,
                DynamicBlockData::LookupAction(_)
                    | DynamicBlockData::LookupParameter(_)
                    | DynamicBlockData::XYParameter(_)
                    | DynamicBlockData::PolarParameter(_)
            ) {
                continue;
            }
            for loaded in [&dwg, &dxf] {
                let Some(ObjectType::DynamicBlock(actual)) = loaded.objects.get(handle) else {
                    panic!("Lost {name} object {handle:?}")
                };
                match (&actual.data, &object.data) {
                    (DynamicBlockData::LookupAction(a), DynamicBlockData::LookupAction(b)) => {
                        assert_eq!((a.row_count, a.column_count), (b.row_count, b.column_count));
                        assert_eq!(a.expressions, b.expressions);
                        assert_eq!(a.columns, b.columns);
                    }
                    (
                        DynamicBlockData::LookupParameter(a),
                        DynamicBlockData::LookupParameter(b),
                    ) => {
                        assert_eq!(
                            (&a.lookup_name, &a.lookup_description, a.index),
                            (&b.lookup_name, &b.lookup_description, b.index)
                        );
                    }
                    (DynamicBlockData::PolarParameter(a), DynamicBlockData::PolarParameter(b)) => {
                        assert_eq!(
                            (
                                &a.distance_name,
                                &a.distance_description,
                                &a.distance_value_set
                            ),
                            (
                                &b.distance_name,
                                &b.distance_description,
                                &b.distance_value_set
                            )
                        );
                        assert_eq!(
                            (&a.angle_name, &a.angle_description, &a.angle_value_set),
                            (&b.angle_name, &b.angle_description, &b.angle_value_set)
                        );
                    }
                    (DynamicBlockData::XYParameter(a), DynamicBlockData::XYParameter(b)) => {
                        assert_eq!(
                            (&a.x_label, &a.x_label_description, &a.x_value_set),
                            (&b.x_label, &b.x_label_description, &b.x_value_set)
                        );
                        assert_eq!(
                            (&a.y_label, &a.y_label_description, &a.y_value_set),
                            (&b.y_label, &b.y_label_description, &b.y_value_set)
                        );
                    }
                    _ => panic!("Wrong object kind {name} {handle:?}"),
                }
            }
        }
    }
}
