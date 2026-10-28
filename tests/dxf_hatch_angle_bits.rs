//! A HATCH's pattern rotation (52) and definition-line angle (53) read back
//! from ASCII DXF with the exact radians they were written with (#132).

use acadrust::entities::hatch::*;
use acadrust::types::{DxfVersion, Vector2};
use acadrust::{CadDocument, DxfReader, DxfWriter, EntityType};
use std::f64::consts::TAU;
use std::io::Cursor;

#[test]
fn hatch_angles_keep_their_bits_through_dxf() {
    let wanted = [0.7853981633974487_f64, 5.4977871437821335_f64];
    for version in [DxfVersion::AC1015, DxfVersion::AC1024, DxfVersion::AC1032] {
        let mut document = CadDocument::new();
        document.version = version;
        let mut hatch = Hatch::with_pattern(HatchPattern::new("ANGLE_CONTROL"));
        hatch.pattern_type = HatchPatternType::Custom;
        hatch.pattern_angle = wanted[0];
        hatch.pattern.lines.push(HatchPatternLine {
            angle: wanted[1],
            base_point: Vector2::ZERO,
            offset: Vector2::new(0.0, 2.0),
            dash_lengths: vec![],
        });
        let mut path = BoundaryPath::new();
        path.edges.push(BoundaryEdge::CircularArc(CircularArcEdge {
            center: Vector2::ZERO,
            radius: 3.0,
            start_angle: 0.0,
            end_angle: TAU,
            counter_clockwise: true,
        }));
        hatch.paths.push(path);
        let id = document
            .add_entity(EntityType::Hatch(Box::new(hatch)))
            .unwrap();
        let bytes = DxfWriter::new(&document).write_to_vec().unwrap();
        let read = DxfReader::from_reader(Cursor::new(bytes))
            .unwrap()
            .read()
            .unwrap();
        let EntityType::Hatch(hatch) = read.get_entity(id).unwrap() else {
            panic!("a hatch");
        };
        assert_eq!(
            hatch.pattern_angle.to_bits(),
            wanted[0].to_bits(),
            "{version:?} 52"
        );
        assert_eq!(
            hatch.pattern.lines[0].angle.to_bits(),
            wanted[1].to_bits(),
            "{version:?} 53"
        );
    }
}
