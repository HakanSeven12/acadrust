//! DWG write must keep the current layer ($CLAYER) when it is given as a name
//! (from DXF, or set programmatically on CadDocument), not only as a handle.
//!
//! The DXF reader stores `$CLAYER` in `header.current_layer_name` only; the DWG
//! writer validates `header.current_layer_handle` and falls back to layer "0"
//! when it is null, so every DXF-to-DWG conversion opens with layer 0 current.

use acadrust::tables::Layer;
use acadrust::types::DxfVersion;
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader};
use std::io::Cursor;

const DXF_CURRENT_LAYER: &str = r#"  0
SECTION
  2
HEADER
  9
$ACADVER
  1
AC1032
  9
$CLAYER
  8
Survey_B
  0
ENDSEC
  0
SECTION
  2
TABLES
  0
TABLE
  2
LAYER
  5
2
 70
2
  0
LAYER
  5
10
  2
0
 70
0
 62
7
  6
Continuous
  0
LAYER
  5
11
  2
Survey_B
 70
0
 62
3
  6
Continuous
  0
ENDTAB
  0
ENDSEC
  0
SECTION
  2
BLOCKS
  0
ENDSEC
  0
SECTION
  2
ENTITIES
  0
ENDSEC
  0
SECTION
  2
OBJECTS
  0
ENDSEC
  0
EOF
"#;

fn roundtrip_dwg(doc: &CadDocument) -> CadDocument {
    let dwg = DwgWriter::write_to_vec(doc).expect("write dwg");
    DwgReader::from_stream(Cursor::new(dwg))
        .read()
        .expect("read dwg")
}

#[test]
fn dxf_to_dwg_keeps_current_layer() {
    let doc = DxfReader::from_reader(Cursor::new(DXF_CURRENT_LAYER))
        .expect("open dxf")
        .read()
        .expect("read dxf");
    assert_eq!(doc.header.current_layer_name, "Survey_B");

    let back = roundtrip_dwg(&doc);
    let survey = back
        .layers
        .iter()
        .find(|l| l.name == "Survey_B")
        .expect("layer Survey_B in the DWG");
    assert_eq!(
        back.header.current_layer_handle, survey.handle,
        "$CLAYER must point to Survey_B in the DWG header"
    );
    assert_eq!(
        back.header.current_layer_name, "Survey_B",
        "current layer name must survive DWG roundtrip"
    );
}

#[test]
fn programmatic_current_layer_name_kept_in_dwg() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let mut layer = Layer::new("Survey_B");
    layer.handle = doc.allocate_handle();
    doc.layers.add(layer).expect("add layer");
    doc.header.current_layer_name = "survey_b".to_string();

    let back = roundtrip_dwg(&doc);
    assert!(
        back.header
            .current_layer_name
            .eq_ignore_ascii_case("Survey_B"),
        "expected Survey_B (case-insensitive), got {}",
        back.header.current_layer_name
    );
}
