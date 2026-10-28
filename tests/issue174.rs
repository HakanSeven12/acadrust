//! Elliptic hatch boundary angles are degrees in DXF and radians in the model.
//!
//! Issue #174: the circular edge converted groups 50/51, and the elliptic edge
//! did not. A file from AutoCAD then swept the wrong arc, while acadrust's own
//! DXF still round-tripped.

use std::f64::consts::PI;
use std::io::Cursor;

use acadrust::entities::{
    BoundaryEdge, BoundaryPath, CircularArcEdge, EllipticArcEdge, EntityType, Hatch,
};
use acadrust::types::Vector2;
use acadrust::{CadDocument, DxfReader, DxfWriter};

fn read_dxf(text: &str) -> CadDocument {
    DxfReader::from_reader(Cursor::new(text.as_bytes().to_vec()))
        .expect("open dxf")
        .read()
        .expect("read dxf")
}

fn with_entities(entities: &str) -> CadDocument {
    let text = String::from_utf8(DxfWriter::new(&CadDocument::new()).write_to_vec().unwrap())
        .unwrap()
        .replace("\r\n", "\n");
    let at = text.find("ENTITIES\n").expect("entities section") + "ENTITIES\n".len();
    read_dxf(&format!("{}{entities}{}", &text[..at], &text[at..]))
}

fn hatch_edges(document: &CadDocument) -> Vec<BoundaryEdge> {
    let mut edges = Vec::new();
    for entity in document.entities() {
        if let EntityType::Hatch(hatch) = entity {
            for path in &hatch.paths {
                edges.extend(path.edges.iter().cloned());
            }
        }
    }
    edges
}

#[test]
fn dxf_elliptic_hatch_edge_angles_are_degrees() {
    let hatch = "\
  0\nHATCH\n  5\nFFFF0\n100\nAcDbEntity\n  8\n0\n100\nAcDbHatch\n\
 10\n0.0\n 20\n0.0\n 30\n0.0\n210\n0.0\n220\n0.0\n230\n1.0\n  2\nSOLID\n\
 70\n1\n 71\n0\n 91\n1\n 92\n1\n 93\n1\n 72\n3\n\
 10\n0.0\n 20\n0.0\n 11\n10.0\n 21\n0.0\n 40\n0.5\n 50\n0.0\n 51\n180.0\n 73\n1\n\
 97\n0\n 75\n0\n 76\n1\n 98\n0\n";
    let edges = hatch_edges(&with_entities(hatch));
    let BoundaryEdge::EllipticArc(edge) = &edges[0] else {
        panic!("expected an elliptic edge");
    };
    assert!((edge.start_angle - 0.0).abs() < 1e-9);
    assert!(
        (edge.end_angle - PI).abs() < 1e-9,
        "end angle {} is still degrees",
        edge.end_angle
    );
    assert!((edge.minor_axis_ratio - 0.5).abs() < 1e-9);
}

#[test]
fn dxf_writer_emits_elliptic_hatch_angles_in_degrees() {
    let mut document = CadDocument::new();
    let mut elliptic = Hatch::new();
    let mut path = BoundaryPath::new();
    path.edges.push(BoundaryEdge::EllipticArc(EllipticArcEdge {
        center: Vector2::new(0.0, 0.0),
        major_axis_endpoint: Vector2::new(10.0, 0.0),
        minor_axis_ratio: 0.5,
        start_angle: 0.0,
        end_angle: PI,
        counter_clockwise: true,
    }));
    elliptic.paths.push(path);
    document
        .add_entity(EntityType::Hatch(Box::new(elliptic)))
        .unwrap();

    let mut circular = Hatch::new();
    let mut path = BoundaryPath::new();
    path.edges.push(BoundaryEdge::CircularArc(CircularArcEdge {
        center: Vector2::new(40.0, 0.0),
        radius: 10.0,
        start_angle: 0.0,
        end_angle: PI,
        counter_clockwise: true,
    }));
    circular.paths.push(path);
    document
        .add_entity(EntityType::Hatch(Box::new(circular)))
        .unwrap();

    let text = String::from_utf8(DxfWriter::new(&document).write_to_vec().unwrap()).unwrap();
    let lines: Vec<_> = text.lines().map(str::trim).collect();
    let mut in_hatch = false;
    let mut edge_type = None;
    let mut ends = Vec::new();
    let mut index = 0;
    while index + 1 < lines.len() {
        let code = lines[index].parse::<i32>().unwrap_or(-1);
        let value = lines[index + 1];
        index += 2;
        if code == 0 {
            in_hatch = value == "HATCH";
            edge_type = None;
            continue;
        }
        if !in_hatch {
            continue;
        }
        if code == 72 {
            edge_type = value
                .parse::<i16>()
                .ok()
                .filter(|kind| (1..=4).contains(kind));
        } else if code == 51 {
            if let Some(kind) = edge_type.take() {
                ends.push((kind, value.parse::<f64>().unwrap()));
            }
        }
    }
    assert_eq!(ends.len(), 2, "edge end angles: {ends:?}");
    for (kind, degrees) in ends {
        assert!(
            (degrees - 180.0).abs() < 1e-6,
            "edge type {kind} wrote {degrees}, expected 180 degrees"
        );
    }
}
