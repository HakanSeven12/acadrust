use crate::entities::modification::mirror::tests::{assert_native_case, NativeMirrorCase};

#[test]
fn large_plane_offsets_do_not_overflow_a_cancelling_dot_product() {
    use super::*;
    let mirror = MirrorPlane::PointNormal {
        point: Vector3::new(f64::MAX, f64::MAX, f64::MAX),
        normal: Vector3::new(1.0, 1.0, -1.0),
    }
    .transform();
    assert!(!mirror.is_identity());
    let reflected = mirror.apply(Vector3::ZERO);
    assert!((reflected.x / f64::MAX - 2.0 / 3.0).abs() < 1e-12);
    assert!((reflected.y / f64::MAX - 2.0 / 3.0).abs() < 1e-12);
    assert!((reflected.z / f64::MAX + 2.0 / 3.0).abs() < 1e-12);
}

#[test]
fn three_point_planes_handle_subnormal_and_overflowing_directions() {
    use super::*;
    let tiny = f64::from_bits(1);
    for plane in [
        MirrorPlane::ThreePoint {
            first: Vector3::ZERO,
            second: Vector3::new(tiny, 0.0, 0.0),
            third: Vector3::new(0.0, tiny, 0.0),
        },
        MirrorPlane::ThreePoint {
            first: Vector3::new(1e308, 0.0, 0.0),
            second: Vector3::new(-1e308, 0.0, 0.0),
            third: Vector3::new(1e308, 1e308, 0.0),
        },
    ] {
        let point = plane.transform().apply(Vector3::new(1.0, 2.0, 3.0));
        assert!((point - Vector3::new(1.0, 2.0, -3.0)).length() < 1e-12);
    }
    for plane in [
        MirrorPlane::ThreePoint {
            first: Vector3::ZERO,
            second: Vector3::UNIT_X,
            third: Vector3::UNIT_X * 2.0,
        },
        MirrorPlane::ThreePoint {
            first: Vector3::ZERO,
            second: Vector3::UNIT_X,
            third: Vector3::new(f64::NAN, 0.0, 0.0),
        },
    ] {
        assert!(plane.transform().is_identity());
    }
}

// Captured using ACAD 2027 native MIRROR3D, separately from planar MIRROR.
const NATIVE_MIRROR3D_CASES: &[NativeMirrorCase] = &[
    NativeMirrorCase { name: "MIRROR3D_TEXT_M0", source: "0\nTEXT\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbText\n10\n3\n20\n5\n30\n0\n40\n2.5\n1\nMirror Test\n50\n22.91831180523293\n41\n0.8\n51\n11.459155902616464\n7\nStandard\n71\n0\n72\n0\n11\n0\n21\n0\n31\n0\n210\n0\n220\n0\n230\n1\n100\nAcDbText\n73\n0\n", expected: "0\nTEXT\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbText\n10\n-21.4212\n20\n12.7884\n30\n0\n40\n2.5\n1\nMirror Test\n50\n337.0819570735708\n41\n0.8\n51\n11.459155902616464\n7\nStandard\n71\n0\n72\n0\n11\n0\n21\n0\n31\n0\n210\n0\n220\n4.44089e-16\n230\n-1\n100\nAcDbText\n73\n0\n" },
    NativeMirrorCase { name: "MIRROR3D_MTEXT_M0", source: "0\nMTEXT\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbMText\n10\n3\n20\n5\n30\n0\n40\n2.5\n41\n30\n46\n0\n71\n1\n72\n5\n1\nFirst\\PSecond\n7\nStandard\n210\n0\n220\n0\n230\n1\n11\n0.921061\n21\n0.389418\n31\n0\n42\n14.1667\n43\n6.66667\n50\n22.91831180523293\n73\n1\n44\n1\n", expected: "0\nMTEXT\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbMText\n10\n3\n20\n5\n30\n2.22045e-15\n40\n2.5\n41\n30\n46\n0\n71\n3\n72\n5\n1\nFirst\\PSecond\n7\nStandard\n210\n0\n220\n4.44089e-16\n230\n-1\n11\n-0.921061\n21\n-0.389418\n31\n-1.72936e-16\n42\n14.1667\n43\n6.66667\n50\n337.0819570735708\n73\n1\n44\n1\n" },
    NativeMirrorCase { name: "MIRROR3D_TEXT_M1", source: "0\nTEXT\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbText\n10\n3\n20\n5\n30\n0\n40\n2.5\n1\nMirror Test\n50\n22.91831180523293\n41\n0.8\n51\n11.459155902616464\n7\nStandard\n71\n0\n72\n0\n11\n0\n21\n0\n31\n0\n210\n0\n220\n0\n230\n1\n100\nAcDbText\n73\n0\n", expected: "0\nTEXT\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbText\n10\n-3\n20\n5\n30\n0\n40\n2.5\n1\nMirror Test\n50\n337.0819570735708\n41\n0.8\n51\n11.459155902616464\n7\nStandard\n71\n2\n72\n0\n11\n0\n21\n0\n31\n0\n210\n0\n220\n4.44089e-16\n230\n-1\n100\nAcDbText\n73\n0\n" },
    NativeMirrorCase { name: "MIRROR3D_MTEXT_M1", source: "0\nMTEXT\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbMText\n10\n3\n20\n5\n30\n0\n40\n2.5\n41\n30\n46\n0\n71\n1\n72\n5\n1\nFirst\\PSecond\n7\nStandard\n210\n0\n220\n0\n230\n1\n11\n0.921061\n21\n0.389418\n31\n0\n42\n14.1667\n43\n6.66667\n50\n22.91831180523293\n73\n1\n44\n1\n", expected: "0\nMTEXT\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbMText\n10\n3\n20\n5\n30\n2.22045e-15\n40\n2.5\n41\n30\n46\n0\n71\n1\n72\n5\n1\nFirst\\PSecond\n7\nStandard\n210\n0\n220\n-4.44089e-16\n230\n1\n11\n0.921061\n21\n0.389418\n31\n1.72936e-16\n42\n14.1667\n43\n6.66667\n50\n22.91831180523293\n73\n1\n44\n1\n" },
    NativeMirrorCase { name: "MIRROR3D_SOLID", source: "0\nSOLID\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbTrace\n10\n1\n20\n2\n30\n3\n11\n10\n21\n2\n31\n3\n12\n1\n22\n12\n32\n3\n13\n10\n23\n12\n33\n3\n39\n2\n210\n0\n220\n0\n230\n1\n", expected: "0\nSOLID\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbTrace\n10\n-1\n20\n2\n30\n3\n11\n-10\n21\n2\n31\n3\n12\n-1\n22\n12\n32\n3\n13\n-10\n23\n12\n33\n3\n39\n2\n210\n0\n220\n4.44089e-16\n230\n-1\n" },
    NativeMirrorCase { name: "MIRROR3D_FACE", source: "0\n3DFACE\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbFace\n10\n1\n20\n2\n30\n3\n11\n10\n21\n2\n31\n3\n12\n10\n22\n12\n32\n3\n13\n1\n23\n12\n33\n3\n70\n1\n", expected: "0\n3DFACE\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbFace\n10\n1\n20\n12\n30\n-3\n11\n10\n21\n12\n31\n-3\n12\n10\n22\n2\n32\n-3\n13\n1\n23\n2\n33\n-3\n70\n4\n" },
    NativeMirrorCase { name: "MIRROR3D_ARC", source: "0\nARC\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbCircle\n10\n2\n20\n3\n30\n5\n40\n4\n210\n0\n220\n0\n230\n1\n100\nAcDbArc\n50\n17.188733853924695\n51\n137.50987083139756\n", expected: "0\nARC\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbCircle\n10\n-2\n20\n3\n30\n5\n40\n4\n210\n0\n220\n4.44089e-16\n230\n-1\n100\nAcDbArc\n50\n42.49014901644526\n51\n162.8111141065796\n" },
    NativeMirrorCase { name: "MIRROR3D_ELLIPSE", source: "0\nELLIPSE\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbEllipse\n10\n2\n20\n3\n30\n5\n11\n4\n21\n0\n31\n0\n210\n0\n220\n0\n230\n1\n40\n0.5\n41\n0.3\n42\n2.4\n", expected: "0\nELLIPSE\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbEllipse\n10\n2\n20\n3\n30\n-5\n11\n4\n21\n0\n31\n0\n210\n0\n220\n-4.44089e-16\n230\n1\n40\n0.5\n41\n0.3\n42\n2.4\n" },
    NativeMirrorCase { name: "MIRROR3D_MESH", source: "0\nMESH\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbSubDMesh\n71\n2\n72\n0\n91\n0\n92\n4\n10\n1\n20\n2\n30\n0\n10\n10\n20\n2\n30\n0\n10\n10\n20\n12\n30\n0\n10\n1\n20\n12\n30\n0\n93\n5\n90\n4\n90\n0\n90\n1\n90\n2\n90\n3\n94\n4\n90\n0\n90\n1\n90\n1\n90\n2\n90\n2\n90\n3\n90\n0\n90\n3\n95\n4\n140\n0\n140\n0\n140\n0\n140\n0\n90\n0\n", expected: "0\nMESH\n100\nAcDbEntity\n67\n0\n8\n0\n100\nAcDbSubDMesh\n71\n2\n72\n0\n91\n0\n92\n4\n10\n1\n20\n2\n30\n8.88178e-16\n10\n10\n20\n2\n30\n8.88178e-16\n10\n10\n20\n12\n30\n5.32907e-15\n10\n1\n20\n12\n30\n5.32907e-15\n93\n5\n90\n4\n90\n0\n90\n1\n90\n2\n90\n3\n94\n4\n90\n0\n90\n1\n90\n1\n90\n2\n90\n2\n90\n3\n90\n0\n90\n3\n95\n4\n140\n0\n140\n0\n140\n0\n140\n0\n90\n0\n" },
];

#[test]
fn native_2027_mirror3d_scenarios() {
    assert_eq!(NATIVE_MIRROR3D_CASES.len(), 9);
    let failures: Vec<_> = NATIVE_MIRROR3D_CASES
        .iter()
        .copied()
        .filter_map(|case| assert_native_case(case).err())
        .collect();
    assert!(
        failures.is_empty(),
        "{} native cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
