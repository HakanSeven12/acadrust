//! A formula written with its operand fields inline is stored the way a
//! drawing keeps one: each operand is a child FIELD the formula refers to as
//! `%<\_FldIdx n>%`.

use std::f64::consts::PI;
use std::io::Cursor;

use acadrust::document::FieldDef;
use acadrust::entities::{Circle, MText};
use acadrust::fields::{self, FieldContext, NewField};
use acadrust::types::{Handle, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfWriter, EntityType};

struct Context;

impl FieldContext for Context {
    fn now_julian(&self) -> f64 {
        2460000.0
    }
}

/// The fields `owner` holds, in handle order.
fn children(doc: &CadDocument, owner: Handle) -> Vec<&FieldDef> {
    let mut kids: Vec<&FieldDef> = doc.fields.values().filter(|f| f.owner == owner).collect();
    kids.sort_by_key(|f| u64::from(f.handle));
    kids
}

/// The formula a text shows: its container's one child.
fn formula(doc: &CadDocument, container: Handle) -> &FieldDef {
    let kids = children(doc, container);
    assert_eq!(kids.len(), 1, "one formula under the container");
    kids[0]
}

fn assert_stored_as_operands(doc: &CadDocument, container: Handle, small: Handle, large: Handle) {
    let formula = formula(doc, container);
    assert_eq!(
        formula.code,
        "\\AcExpr (%<\\_FldIdx 0>%+%<\\_FldIdx 1>%) \\f \"%lu2%pr2\""
    );
    assert!(formula.objects.is_empty(), "{:?}", formula.objects);
    let operands: Vec<(&str, &[Handle])> = children(doc, formula.handle)
        .iter()
        .map(|f| (f.code.as_str(), f.objects.as_slice()))
        .collect();
    let area = "\\AcObjProp Object(%<\\_ObjIdx 0>%).Area";
    assert_eq!(operands, [(area, &[small][..]), (area, &[large][..])]);
}

#[test]
fn inline_operands_are_stored_as_child_fields() {
    let mut doc = CadDocument::new();
    let circle = |r: f64| EntityType::Circle(Circle::from_center_radius(Vector3::ZERO, r));
    let small = doc.add_entity(circle(1.0)).unwrap();
    let large = doc.add_entity(circle(2.0)).unwrap();
    let host = doc
        .add_entity(EntityType::MText(Box::new(MText::new())))
        .unwrap();
    let mut sum = NewField::new(
        "\\AcExpr (%<\\AcObjProp Object(%<\\_ObjIdx 0>%).Area>%+%<\\AcObjProp Object(%<\\_ObjIdx 1>%).Area>%) \\f \"%lu2%pr2\"",
        "15.71",
    );
    sum.objects = vec![small, large];
    let container = doc
        .set_text_field(host, "%<\\_FldIdx 0>%", vec![sum])
        .unwrap();

    let expected = format!("{:.2}", 5.0 * PI);
    assert_stored_as_operands(&doc, container, small, large);
    assert_eq!(
        fields::resolve(&doc, host, &Context).as_deref(),
        Some(expected.as_str())
    );

    let dxf = DxfWriter::new(&doc).write_to_vec().unwrap();
    let dwg = DwgWriter::write_to_vec(&doc).unwrap();
    for loaded in [
        DxfReader::from_reader(Cursor::new(dxf))
            .unwrap()
            .read()
            .unwrap(),
        DwgReader::from_stream(Cursor::new(dwg)).read().unwrap(),
    ] {
        assert_stored_as_operands(&loaded, container, small, large);
        assert_eq!(
            fields::resolve(&loaded, host, &Context).as_deref(),
            Some(expected.as_str())
        );
    }
}
