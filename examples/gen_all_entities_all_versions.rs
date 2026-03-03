/// Generate DWG files for every supported entity type × every DWG version.
///
/// Output structure:  target/entities_dwg/<VERSION>/entity_<VERSION>_<TYPE>.dwg
///
/// Versions: AC1012 (R13), AC1014 (R14), AC1015 (R2000), AC1018 (R2004),
///           AC1021 (R2007), AC1024 (R2010), AC1027 (R2013), AC1032 (R2018)
///
/// Some entity types are only available from certain versions onwards.
/// MESH and MULTILEADER require class-based type codes (R2000+).

use acadrust::entities::*;
use acadrust::entities::dimension::DimensionLinear;
use acadrust::entities::hatch::{
    BoundaryEdge, BoundaryPath, BoundaryPathFlags, LineEdge, PolylineEdge,
};
use acadrust::entities::mesh::Mesh;
use acadrust::entities::mline::MLine;
use acadrust::entities::multileader::MultiLeader;
use acadrust::entities::polyface_mesh::PolyfaceMesh;
use acadrust::types::{DxfVersion, Vector2, Vector3};
use acadrust::{CadDocument, DwgWriter};

/// All DWG versions to test
const VERSIONS: &[(DxfVersion, &str)] = &[
    (DxfVersion::AC1012, "AC1012"),
    (DxfVersion::AC1014, "AC1014"),
    (DxfVersion::AC1015, "AC1015"),
    (DxfVersion::AC1018, "AC1018"),
    (DxfVersion::AC1021, "AC1021"),
    (DxfVersion::AC1024, "AC1024"),
    (DxfVersion::AC1027, "AC1027"),
    (DxfVersion::AC1032, "AC1032"),
];

fn main() {
    let root = "target/entities_dwg";
    let mut total_ok = 0u32;
    let mut total_fail = 0u32;
    let mut total_skip = 0u32;

    for &(version, ver_str) in VERSIONS {
        let dir = format!("{}/{}", root, ver_str);
        std::fs::create_dir_all(&dir).unwrap();

        let mut ok = 0u32;
        let mut fail = 0u32;
        let mut skip = 0u32;

        println!("\n╔══════════════════════════════════════════════════════╗");
        println!("║  {} ({})                                    ║", ver_str, version_label(version));
        println!("╚══════════════════════════════════════════════════════╝");

        // ── Simple geometry (all versions) ──────────────────────────

        gen(version, ver_str, &dir, "POINT", &mut ok, &mut fail, &mut skip, || {
            EntityType::Point(Point::from_coords(50.0, 50.0, 0.0))
        });

        gen(version, ver_str, &dir, "LINE", &mut ok, &mut fail, &mut skip, || {
            EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 100.0, 100.0, 0.0))
        });

        gen(version, ver_str, &dir, "CIRCLE", &mut ok, &mut fail, &mut skip, || {
            EntityType::Circle(Circle::from_coords(50.0, 50.0, 0.0, 25.0))
        });

        gen(version, ver_str, &dir, "ARC", &mut ok, &mut fail, &mut skip, || {
            EntityType::Arc(Arc::from_coords(50.0, 50.0, 0.0, 25.0, 0.0, std::f64::consts::PI))
        });

        gen(version, ver_str, &dir, "ELLIPSE", &mut ok, &mut fail, &mut skip, || {
            EntityType::Ellipse(Ellipse::from_center_axes(
                Vector3::new(50.0, 50.0, 0.0),
                Vector3::new(40.0, 0.0, 0.0),
                0.5,
            ))
        });

        gen(version, ver_str, &dir, "RAY", &mut ok, &mut fail, &mut skip, || {
            EntityType::Ray(Ray::new(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 0.0),
            ))
        });

        gen(version, ver_str, &dir, "XLINE", &mut ok, &mut fail, &mut skip, || {
            EntityType::XLine(XLine::new(
                Vector3::new(50.0, 50.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
            ))
        });

        // ── Solid / Surface (all versions) ──────────────────────────

        gen(version, ver_str, &dir, "SOLID", &mut ok, &mut fail, &mut skip, || {
            EntityType::Solid(Solid::new(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(10.0, 0.0, 0.0),
                Vector3::new(10.0, 10.0, 0.0),
                Vector3::new(0.0, 10.0, 0.0),
            ))
        });

        gen(version, ver_str, &dir, "FACE3D", &mut ok, &mut fail, &mut skip, || {
            EntityType::Face3D(Face3D::new(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(10.0, 0.0, 0.0),
                Vector3::new(10.0, 10.0, 5.0),
                Vector3::new(0.0, 10.0, 5.0),
            ))
        });

        // ── Text (all versions) ─────────────────────────────────────

        gen(version, ver_str, &dir, "TEXT", &mut ok, &mut fail, &mut skip, || {
            EntityType::Text(Text::with_value("Hello World", Vector3::new(0.0, 0.0, 0.0)))
        });

        gen(version, ver_str, &dir, "MTEXT", &mut ok, &mut fail, &mut skip, || {
            EntityType::MText(MText::with_value("Multi\\Pline\\PText", Vector3::new(0.0, 0.0, 0.0)))
        });

        // ── Polylines (all versions) ────────────────────────────────

        gen(version, ver_str, &dir, "LWPOLYLINE", &mut ok, &mut fail, &mut skip, || {
            EntityType::LwPolyline(LwPolyline::from_points(vec![
                Vector2::new(0.0, 0.0),
                Vector2::new(10.0, 0.0),
                Vector2::new(10.0, 10.0),
                Vector2::new(0.0, 10.0),
            ]))
        });

        gen(version, ver_str, &dir, "POLYLINE2D", &mut ok, &mut fail, &mut skip, || {
            let mut pl = Polyline2D::new();
            pl.add_vertex(Vertex2D::new(Vector3::new(0.0, 0.0, 0.0)));
            pl.add_vertex(Vertex2D::new(Vector3::new(20.0, 0.0, 0.0)));
            pl.add_vertex(Vertex2D::new(Vector3::new(20.0, 20.0, 0.0)));
            EntityType::Polyline2D(pl)
        });

        gen(version, ver_str, &dir, "POLYLINE3D", &mut ok, &mut fail, &mut skip, || {
            EntityType::Polyline3D(Polyline3D::from_points(vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(10.0, 0.0, 5.0),
                Vector3::new(20.0, 10.0, 10.0),
            ]))
        });

        gen(version, ver_str, &dir, "SPLINE", &mut ok, &mut fail, &mut skip, || {
            EntityType::Spline(Spline::from_control_points(3, vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(5.0, 10.0, 0.0),
                Vector3::new(10.0, 0.0, 0.0),
                Vector3::new(15.0, 10.0, 0.0),
            ]))
        });

        // ── Annotations (all versions) ──────────────────────────────

        gen(version, ver_str, &dir, "LEADER", &mut ok, &mut fail, &mut skip, || {
            EntityType::Leader(Leader::two_point(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(10.0, 10.0, 0.0),
            ))
        });

        gen(version, ver_str, &dir, "DIMENSION", &mut ok, &mut fail, &mut skip, || {
            EntityType::Dimension(Dimension::Linear(DimensionLinear::new(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(100.0, 0.0, 0.0),
            )))
        });

        gen(version, ver_str, &dir, "TOLERANCE", &mut ok, &mut fail, &mut skip, || {
            EntityType::Tolerance(Tolerance::with_text(
                Vector3::new(10.0, 10.0, 0.0),
                "{\\Fgdt;p}%%v0.5",
            ))
        });

        gen(version, ver_str, &dir, "SHAPE", &mut ok, &mut fail, &mut skip, || {
            EntityType::Shape(Shape::with_name(
                Vector3::new(50.0, 50.0, 0.0),
                "BOX",
                5.0,
            ))
        });

        gen(version, ver_str, &dir, "VIEWPORT", &mut ok, &mut fail, &mut skip, || {
            EntityType::Viewport(Viewport::new())
        });

        gen(version, ver_str, &dir, "INSERT", &mut ok, &mut fail, &mut skip, || {
            EntityType::Insert(Insert::new("*Model_Space", Vector3::new(0.0, 0.0, 0.0)))
        });

        // ── Hatch (all versions) ────────────────────────────────────

        gen(version, ver_str, &dir, "HATCH_SOLID", &mut ok, &mut fail, &mut skip, || {
            let mut hatch = Hatch::solid();
            let mut path = BoundaryPath::new();
            path.add_edge(BoundaryEdge::Polyline(PolylineEdge::new(
                vec![
                    Vector2::new(0.0, 0.0),
                    Vector2::new(100.0, 0.0),
                    Vector2::new(100.0, 100.0),
                    Vector2::new(0.0, 100.0),
                ],
                true,
            )));
            hatch.add_path(path);
            EntityType::Hatch(hatch)
        });

        gen(version, ver_str, &dir, "HATCH_LINES", &mut ok, &mut fail, &mut skip, || {
            let mut hatch = Hatch::solid();
            let mut path = BoundaryPath::with_flags(BoundaryPathFlags::new());
            path.add_edge(BoundaryEdge::Line(LineEdge {
                start: Vector2::new(0.0, 0.0),
                end: Vector2::new(50.0, 0.0),
            }));
            path.add_edge(BoundaryEdge::Line(LineEdge {
                start: Vector2::new(50.0, 0.0),
                end: Vector2::new(50.0, 50.0),
            }));
            path.add_edge(BoundaryEdge::Line(LineEdge {
                start: Vector2::new(50.0, 50.0),
                end: Vector2::new(0.0, 0.0),
            }));
            hatch.add_path(path);
            EntityType::Hatch(hatch)
        });

        // ── MLine (all versions) ────────────────────────────────────

        gen(version, ver_str, &dir, "MLINE", &mut ok, &mut fail, &mut skip, || {
            EntityType::MLine(MLine::from_points(&[
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(50.0, 0.0, 0.0),
                Vector3::new(50.0, 50.0, 0.0),
            ]))
        });

        // ── PolyfaceMesh (all versions) ─────────────────────────────

        gen(version, ver_str, &dir, "POLYFACE", &mut ok, &mut fail, &mut skip, || {
            let mut pf = PolyfaceMesh::new();
            let v1 = pf.add_vertex_xyz(0.0, 0.0, 0.0);
            let v2 = pf.add_vertex_xyz(10.0, 0.0, 0.0);
            let v3 = pf.add_vertex_xyz(5.0, 10.0, 0.0);
            let v4 = pf.add_vertex_xyz(10.0, 10.0, 5.0);
            pf.add_triangle(v1, v2, v3);
            pf.add_triangle(v2, v4, v3);
            EntityType::PolyfaceMesh(pf)
        });

        // ── MultiLeader (R2000+ / class-based) ─────────────────────

        gen(version, ver_str, &dir, "MULTILEADER", &mut ok, &mut fail, &mut skip, || {
            EntityType::MultiLeader(MultiLeader::with_text(
                "Label",
                Vector3::new(20.0, 20.0, 0.0),
                vec![Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 10.0, 0.0)],
            ))
        });

        // ── Mesh (R2000+ / class-based) ─────────────────────────────

        gen(version, ver_str, &dir, "MESH", &mut ok, &mut fail, &mut skip, || {
            EntityType::Mesh(Mesh::from_triangles(
                vec![
                    Vector3::new(0.0, 0.0, 0.0),
                    Vector3::new(10.0, 0.0, 0.0),
                    Vector3::new(5.0, 10.0, 5.0),
                ],
                &[(0, 1, 2)],
            ))
        });

        // ── Version summary ─────────────────────────────────────────

        println!("  ── {} summary: {} OK, {} FAIL, {} SKIP", ver_str, ok, fail, skip);
        total_ok += ok;
        total_fail += fail;
        total_skip += skip;
    }

    // ── Grand total ─────────────────────────────────────────────────

    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║  GRAND TOTAL: {} OK, {} FAIL, {} SKIP               ║", total_ok, total_fail, total_skip);
    println!("╚══════════════════════════════════════════════════════╝");

    if total_fail > 0 {
        std::process::exit(1);
    }
}

fn version_label(v: DxfVersion) -> &'static str {
    match v {
        DxfVersion::AC1012 => "R13",
        DxfVersion::AC1014 => "R14",
        DxfVersion::AC1015 => "R2000",
        DxfVersion::AC1018 => "R2004",
        DxfVersion::AC1021 => "R2007",
        DxfVersion::AC1024 => "R2010",
        DxfVersion::AC1027 => "R2013",
        DxfVersion::AC1032 => "R2018",
        _ => "???",
    }
}

fn gen<F>(
    version: DxfVersion,
    ver_str: &str,
    dir: &str,
    name: &str,
    ok: &mut u32,
    fail: &mut u32,
    _skip: &mut u32,
    make_entity: F,
)
where
    F: FnOnce() -> EntityType,
{
    let path = format!("{}/entity_{}_{}.dwg", dir, ver_str, name);
    let mut doc = CadDocument::with_version(version);
    let entity = make_entity();
    if let Err(e) = doc.add_entity(entity) {
        println!("  SKIP {:<20} add_entity error: {:?}", name, e);
        *_skip += 1;
        return;
    }
    match DwgWriter::write_to_file(&path, &doc) {
        Ok(()) => {
            let sz = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            println!("  OK   {:<20} ({} bytes)", name, sz);
            *ok += 1;
        }
        Err(e) => {
            println!("  FAIL {:<20} {:?}", name, e);
            *fail += 1;
        }
    }
}
