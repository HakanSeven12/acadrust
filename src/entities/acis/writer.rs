//! SAT (Save and Restore) text format writer.
//!
//! Writes a [`SatDocument`] structure back to ACIS SAT text format.

use super::types::*;

/// Writer for ACIS SAT text format.
pub struct SatWriter;

impl SatWriter {
    /// Write a [`SatDocument`] to SAT text format.
    pub fn write(doc: &SatDocument) -> String {
        let mut output = String::new();

        // Header line 1: version, num_records (always 0 for v7+), num_bodies, has_history
        let num_records_out = if doc.header.version.has_explicit_indices() {
            0 // ACIS 7.0+ always writes 0 for record count
        } else {
            doc.header.num_records
        };
        output.push_str(&format!(
            "{} {} {} {}\n",
            doc.header.version.sat_version_number(),
            num_records_out,
            doc.header.num_bodies,
            if doc.header.has_history { 1 } else { 0 }
        ));

        // Header line 2: product info
        if doc.header.version.has_counted_strings() {
            // ACIS 7.0+ format with @-prefixed counted strings
            output.push_str(&format!(
                "@{} {} @{} {} @{} {}\n",
                doc.header.product_id.len(),
                doc.header.product_id,
                doc.header.product_version.len(),
                doc.header.product_version,
                doc.header.date.len(),
                doc.header.date,
            ));
        } else {
            // Legacy format with length-prefixed strings
            output.push_str(&format!(
                "{} {} {} {} {} {}\n",
                doc.header.product_id.len(),
                doc.header.product_id,
                doc.header.product_version.len(),
                doc.header.product_version,
                doc.header.date.len(),
                doc.header.date,
            ));
        }

        // Header line 3: tolerances
        if let Some(resfit) = doc.header.resfit_tolerance {
            output.push_str(&format!(
                "{} {} {}\n",
                format_float(doc.header.spatial_resolution),
                format_float(doc.header.normal_tolerance),
                format_float(resfit),
            ));
        } else {
            output.push_str(&format!(
                "{} {}\n",
                format_float(doc.header.spatial_resolution),
                format_float(doc.header.normal_tolerance),
            ));
        }

        // Entity records
        for record in &doc.records {
            Self::write_record(&mut output, record, &doc.header.version);
        }

        // End marker
        output.push_str("End-of-ACIS-data\n");

        output
    }

    /// Write a single entity record.
    fn write_record(output: &mut String, record: &SatRecord, version: &SatVersion) {
        // Entity type (no explicit index prefix — ACIS 7.0+ doesn't use them in DXF)
        output.push_str(&record.entity_type);
        output.push(' ');

        // Attribute pointer
        output.push_str(&format!("{}", record.attribute));

        // Subtype/ID field (only present in ACIS 7.0+ / SAT version 700+)
        if version.major >= 7 {
            output.push_str(&format!(" {}", record.subtype_id));
        }

        // Remaining tokens
        for token in &record.tokens {
            output.push(' ');
            Self::write_token(output, token, version);
        }

        // Record terminator
        output.push_str(" #\n");
    }

    /// Write a single token.
    fn write_token(output: &mut String, token: &SatToken, version: &SatVersion) {
        match token {
            SatToken::String(s) => {
                if version.has_counted_strings() {
                    output.push_str(&format!("@{} {}", s.len(), s));
                } else {
                    output.push_str(&format!("{} {}", s.len(), s));
                }
            }
            SatToken::Float(v) => {
                output.push_str(&format_float(*v));
            }
            _ => {
                output.push_str(&format!("{}", token));
            }
        }
    }
}

/// Format a float value for SAT output, preserving precision.
///
/// Uses scientific notation (e.g. `9.9999999999999995e-007`) for very
/// small or very large values, matching the ACIS convention.
fn format_float(v: f64) -> String {
    if v == 0.0 {
        "0".to_string()
    } else if v.fract() == 0.0 && v.abs() < 1e15 && !v.is_infinite() && !v.is_nan() {
        format!("{}", v as i64)
    } else if v.abs() < 1e-3 || v.abs() >= 1e15 {
        // Scientific notation with 3-digit exponent to match ACIS convention.
        // Rust's {:e} gives e.g. "1e-6"; we need "1e-006".
        let s = format!("{:e}", v);
        // Parse exponent part and reformat with 3-digit exponent
        if let Some(pos) = s.find('e') {
            let mantissa = &s[..pos];
            let exp_str = &s[pos + 1..];
            let exp: i32 = exp_str.parse().unwrap_or(0);
            // Trim trailing zeros from mantissa (keep at least one digit
            // after decimal point if present), but strip the point
            // entirely if there is no fractional part.
            let mantissa = if mantissa.contains('.') {
                let trimmed = mantissa.trim_end_matches('0');
                let trimmed = trimmed.trim_end_matches('.');
                trimmed
            } else {
                mantissa
            };
            if exp < 0 {
                format!("{}e-{:03}", mantissa, -exp)
            } else {
                format!("{}e+{:03}", mantissa, exp)
            }
        } else {
            s
        }
    } else {
        // Use full precision
        format!("{}", v)
    }
}

// ============================================================================
// Builder helpers
// ============================================================================

impl SatDocument {
    /// Create a new SAT document for a simple body with ACIS 7.0 format.
    ///
    /// Sets up the `body` record at index 0 (no asmheader needed for DXF SAT).
    pub fn new_body() -> Self {
        let mut doc = Self::new();
        doc.header.num_bodies = 1;

        // Add body (index 0 — SAT in DXF does not use asmheader)
        let mut body = SatRecord::new(0, "body");
        body.attribute = SatPointer::NULL;
        body.tokens.push(SatToken::Pointer(SatPointer::NULL)); // next_body
        body.tokens.push(SatToken::Pointer(SatPointer::NULL)); // lump
        body.tokens.push(SatToken::Pointer(SatPointer::NULL)); // wire
        body.tokens.push(SatToken::Pointer(SatPointer::NULL)); // transform
        doc.records.push(body);

        doc.header.num_records = doc.records.len();
        doc
    }

    /// Add a transform record and return its index.
    pub fn add_transform(
        &mut self,
        rotation: [[f64; 3]; 3],
        translation: [f64; 3],
        scale: f64,
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "transform");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)

        // 3x3 rotation matrix
        for row in &rotation {
            for &val in row {
                record.tokens.push(SatToken::Float(val));
            }
        }

        // Translation
        for &val in &translation {
            record.tokens.push(SatToken::Float(val));
        }

        // Scale
        record.tokens.push(SatToken::Float(scale));

        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a point record and return its index.
    pub fn add_point(&mut self, x: f64, y: f64, z: f64) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "point");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        record.tokens.push(SatToken::Float(x));
        record.tokens.push(SatToken::Float(y));
        record.tokens.push(SatToken::Float(z));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a plane-surface record and return its index.
    pub fn add_plane_surface(
        &mut self,
        root: [f64; 3],
        normal: [f64; 3],
        u_dir: [f64; 3],
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "plane-surface");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        for &v in &root {
            record.tokens.push(SatToken::Float(v));
        }
        for &v in &normal {
            record.tokens.push(SatToken::Float(v));
        }
        for &v in &u_dir {
            record.tokens.push(SatToken::Float(v));
        }
        record.tokens.push(SatToken::Ident("forward_v".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a straight-curve record and return its index.
    pub fn add_straight_curve(
        &mut self,
        root: [f64; 3],
        direction: [f64; 3],
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "straight-curve");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        for &v in &root {
            record.tokens.push(SatToken::Float(v));
        }
        for &v in &direction {
            record.tokens.push(SatToken::Float(v));
        }
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a cone-surface record and return its index.
    ///
    /// For a cylinder, use `cos_half_angle = 1.0` and `sin_half_angle = 0.0`.
    pub fn add_cone_surface(
        &mut self,
        center: [f64; 3],
        axis: [f64; 3],
        major_axis: [f64; 3],
        ratio: f64,
        cos_half_angle: f64,
        sin_half_angle: f64,
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "cone-surface");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        for &v in &center {
            record.tokens.push(SatToken::Float(v));
        }
        for &v in &axis {
            record.tokens.push(SatToken::Float(v));
        }
        for &v in &major_axis {
            record.tokens.push(SatToken::Float(v));
        }
        record.tokens.push(SatToken::Float(ratio));
        // Spline/law continuation parameters (always I I for simple surfaces)
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        // Half-angle: ACIS stores sine first, then cosine
        record.tokens.push(SatToken::Float(sin_half_angle));
        record.tokens.push(SatToken::Float(cos_half_angle));
        // Radius at reference cross-section = |major_axis|
        let radius = (major_axis[0] * major_axis[0]
            + major_axis[1] * major_axis[1]
            + major_axis[2] * major_axis[2])
        .sqrt();
        record.tokens.push(SatToken::Float(radius));
        // cone-surface uses `forward` (not `forward_v` like plane/sphere)
        record.tokens.push(SatToken::Ident("forward".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a sphere-surface record and return its index.
    pub fn add_sphere_surface(
        &mut self,
        center: [f64; 3],
        radius: f64,
        u_dir: [f64; 3],
        pole: [f64; 3],
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "sphere-surface");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        for &v in &center {
            record.tokens.push(SatToken::Float(v));
        }
        record.tokens.push(SatToken::Float(radius));
        for &v in &u_dir {
            record.tokens.push(SatToken::Float(v));
        }
        for &v in &pole {
            record.tokens.push(SatToken::Float(v));
        }
        record.tokens.push(SatToken::Ident("forward_v".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a torus-surface record and return its index.
    pub fn add_torus_surface(
        &mut self,
        center: [f64; 3],
        normal: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
        u_dir: [f64; 3],
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "torus-surface");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        for &v in &center {
            record.tokens.push(SatToken::Float(v));
        }
        for &v in &normal {
            record.tokens.push(SatToken::Float(v));
        }
        record.tokens.push(SatToken::Float(major_radius));
        record.tokens.push(SatToken::Float(minor_radius));
        for &v in &u_dir {
            record.tokens.push(SatToken::Float(v));
        }
        record.tokens.push(SatToken::Ident("forward_v".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add an ellipse-curve record and return its index.
    pub fn add_ellipse_curve(
        &mut self,
        center: [f64; 3],
        normal: [f64; 3],
        major_axis: [f64; 3],
        ratio: f64,
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "ellipse-curve");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        for &v in &center {
            record.tokens.push(SatToken::Float(v));
        }
        for &v in &normal {
            record.tokens.push(SatToken::Float(v));
        }
        for &v in &major_axis {
            record.tokens.push(SatToken::Float(v));
        }
        record.tokens.push(SatToken::Float(ratio));
        record.tokens.push(SatToken::Ident("I".to_string()));
        record.tokens.push(SatToken::Ident("I".to_string()));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a vertex record and return its index.
    pub fn add_vertex(&mut self, edge: SatPointer, point: SatPointer) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "vertex");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        record.tokens.push(SatToken::Pointer(edge));
        record.tokens.push(SatToken::Pointer(point));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add an edge record and return its index.
    ///
    /// The v700 edge format:
    /// `edge $attr -1 $-1 $start_v start_param $end_v end_param $coedge $curve sense @7 unknown`
    pub fn add_edge(
        &mut self,
        start_vertex: SatPointer,
        start_param: f64,
        end_vertex: SatPointer,
        end_param: f64,
        coedge: SatPointer,
        curve: SatPointer,
        sense: Sense,
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "edge");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        record.tokens.push(SatToken::Pointer(start_vertex));
        record.tokens.push(SatToken::Float(start_param));
        record.tokens.push(SatToken::Pointer(end_vertex));
        record.tokens.push(SatToken::Float(end_param));
        record.tokens.push(SatToken::Pointer(coedge));
        record.tokens.push(SatToken::Pointer(curve));
        record.tokens.push(SatToken::Enum(sense.as_str().to_string()));
        record.tokens.push(SatToken::String("unknown".to_string()));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a coedge record and return its index.
    pub fn add_coedge(
        &mut self,
        next: SatPointer,
        prev: SatPointer,
        partner: SatPointer,
        edge: SatPointer,
        sense: Sense,
        owner_loop: SatPointer,
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "coedge");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // v700 unknown (always $-1)
        record.tokens.push(SatToken::Pointer(next));
        record.tokens.push(SatToken::Pointer(prev));
        record.tokens.push(SatToken::Pointer(partner));
        record.tokens.push(SatToken::Pointer(edge));
        record.tokens.push(SatToken::Enum(sense.as_str().to_string()));
        record.tokens.push(SatToken::Pointer(owner_loop));
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // pcurve (always $-1)
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a loop record and return its index.
    pub fn add_loop(
        &mut self,
        next_loop: SatPointer,
        first_coedge: SatPointer,
        face: SatPointer,
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "loop");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // unknown (always $-1)
        record.tokens.push(SatToken::Pointer(next_loop));
        record.tokens.push(SatToken::Pointer(first_coedge));
        record.tokens.push(SatToken::Pointer(face));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a face record and return its index.
    pub fn add_face(
        &mut self,
        next_face: SatPointer,
        first_loop: SatPointer,
        shell: SatPointer,
        surface: SatPointer,
        sense: Sense,
        sidedness: Sidedness,
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "face");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // unknown (always $-1)
        record.tokens.push(SatToken::Pointer(next_face));
        record.tokens.push(SatToken::Pointer(first_loop));
        record.tokens.push(SatToken::Pointer(shell));
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // subshell
        record.tokens.push(SatToken::Pointer(surface));
        record.tokens.push(SatToken::Enum(sense.as_str().to_string()));
        record.tokens.push(SatToken::Enum(sidedness.as_str().to_string()));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a shell record and return its index.
    pub fn add_shell(
        &mut self,
        first_face: SatPointer,
        lump: SatPointer,
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "shell");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // next_shell
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // subshell
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // unknown (always $-1)
        record.tokens.push(SatToken::Pointer(first_face));
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // wire
        record.tokens.push(SatToken::Pointer(lump));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }

    /// Add a lump record and return its index.
    pub fn add_lump(
        &mut self,
        shell: SatPointer,
        body: SatPointer,
    ) -> i32 {
        let index = self.records.len() as i32;
        let mut record = SatRecord::new(index, "lump");
        record.attribute = SatPointer::NULL;
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // next_lump
        record.tokens.push(SatToken::Pointer(SatPointer::NULL)); // unknown (always $-1)
        record.tokens.push(SatToken::Pointer(shell));
        record.tokens.push(SatToken::Pointer(body));
        self.records.push(record);
        self.header.num_records = self.records.len();
        index
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_empty_body() {
        let doc = SatDocument::new_body();
        let output = doc.to_sat_string();

        assert!(output.contains("700"));
        assert!(output.contains("body"));
        assert!(output.contains("End-of-ACIS-data"));
    }

    #[test]
    fn test_roundtrip_simple() {
        let original = "700 0 1 0\n\
            @8 acadrust @8 ACIS 7.0 @24 Thu Jan 01 00:00:00 2023\n\
            1e-06 9.9999999999999995e-07\n\
            -0 asmheader $-1 -1 @12 700 7 0 0 @5 ACIS @3 7.0 @24 Thu Jan 01 00:00:00 2023 #\n\
            -1 body $-1 $-1 $-1 $-1 #\n\
            End-of-ACIS-data\n";

        let doc = SatDocument::parse(original).unwrap();
        let output = doc.to_sat_string();

        // Parse the output again
        let doc2 = SatDocument::parse(&output).unwrap();
        assert_eq!(doc.records.len(), doc2.records.len());
        assert_eq!(doc.header.version, doc2.header.version);

        for (r1, r2) in doc.records.iter().zip(doc2.records.iter()) {
            assert_eq!(r1.entity_type, r2.entity_type);
            assert_eq!(r1.index, r2.index);
        }
    }

    #[test]
    fn test_roundtrip_v400() {
        let original = "400 0 1 0\n\
            8 acadrust 8 ACIS 4.0 24 Thu Jan 01 00:00:00 2023\n\
            1e-06 9.9999999999999995e-07\n\
            body $-1 $1 $-1 $-1 #\n\
            lump $-1 $-1 $2 $0 #\n\
            shell $-1 $-1 $-1 $3 $-1 $1 #\n\
            End-of-ACIS-data\n";

        let doc = SatDocument::parse(original).unwrap();
        assert_eq!(doc.header.version, SatVersion::new(4, 0, 0));

        let output = doc.to_sat_string();
        let doc2 = SatDocument::parse(&output).unwrap();
        assert_eq!(doc.records.len(), doc2.records.len());
    }

    #[test]
    fn test_write_with_geometry() {
        let mut doc = SatDocument::new_body();

        // Add a plane surface
        let plane_idx = doc.add_plane_surface(
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
        );

        let output = doc.to_sat_string();
        assert!(output.contains("plane-surface"));

        // Parse back and verify
        let doc2 = SatDocument::parse(&output).unwrap();
        let planes = doc2.records_of_type("plane-surface");
        assert_eq!(planes.len(), 1);
        let plane = SatPlaneSurface::from_record(planes[0]).unwrap();
        assert_eq!(plane.root_point(), (0.0, 0.0, 5.0));
        assert_eq!(plane.normal(), (0.0, 0.0, 1.0));

        assert!(plane_idx >= 0);
    }

    #[test]
    fn test_build_topology() {
        let mut doc = SatDocument::new_body();
        let body_idx = 0; // body is at index 0

        // Build minimal topology
        let point_idx = doc.add_point(1.0, 2.0, 3.0);
        let vertex_idx = doc.add_vertex(SatPointer::NULL, SatPointer::new(point_idx));
        let surface_idx = doc.add_plane_surface(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
        );

        let output = doc.to_sat_string();
        assert!(output.contains("point"));
        assert!(output.contains("vertex"));
        assert!(output.contains("plane-surface"));

        // Verify indices
        assert!(vertex_idx > point_idx);
        assert!(surface_idx > vertex_idx);
        assert!(body_idx >= 0);
    }

    #[test]
    fn test_float_formatting() {
        assert_eq!(format_float(0.0), "0");
        assert_eq!(format_float(1.0), "1");
        assert_eq!(format_float(-5.0), "-5");
        assert_eq!(format_float(1e-06), "1e-006");
    }

    #[test]
    fn test_add_sphere_surface() {
        let mut doc = SatDocument::new_body();
        let idx = doc.add_sphere_surface(
            [0.0, 0.0, 0.0],
            5.0,
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
        );

        let output = doc.to_sat_string();
        assert!(output.contains("sphere-surface"));

        let doc2 = SatDocument::parse(&output).unwrap();
        let spheres = doc2.records_of_type("sphere-surface");
        assert_eq!(spheres.len(), 1);
        let sphere = SatSphereSurface::from_record(spheres[0]).unwrap();
        assert_eq!(sphere.center(), (0.0, 0.0, 0.0));
        assert_eq!(sphere.radius(), 5.0);

        assert!(idx >= 0);
    }

    #[test]
    fn test_cone_surface_accessor_positions() {
        let mut doc = SatDocument::new_body();
        let _idx = doc.add_cone_surface(
            [1.0, 2.0, 3.0],      // center
            [0.0, 0.0, 1.0],      // axis
            [5.0, 0.0, 0.0],      // major_axis (radius = 5)
            1.0,                    // ratio
            1.0,                    // cos_half_angle (cylinder)
            0.0,                    // sin_half_angle (cylinder)
        );

        // Roundtrip through SAT text
        let output = doc.to_sat_string();
        let doc2 = SatDocument::parse(&output).unwrap();
        let cones = doc2.records_of_type("cone-surface");
        assert_eq!(cones.len(), 1);
        let cone = SatConeSurface::from_record(cones[0]).unwrap();
        assert_eq!(cone.center(), (1.0, 2.0, 3.0));
        assert_eq!(cone.axis(), (0.0, 0.0, 1.0));
        assert_eq!(cone.major_axis(), (5.0, 0.0, 0.0));
        assert_eq!(cone.ratio(), 1.0);
        assert!((cone.sin_half_angle() - 0.0).abs() < 1e-10, "sin should be 0.0 for cylinder, got {}", cone.sin_half_angle());
        assert!((cone.cos_half_angle() - 1.0).abs() < 1e-10, "cos should be 1.0 for cylinder, got {}", cone.cos_half_angle());
        assert!((cone.radius() - 5.0).abs() < 1e-10, "radius should be 5.0, got {}", cone.radius());
    }

    #[test]
    fn test_cone_surface_accessor_with_cone_angles() {
        let mut doc = SatDocument::new_body();
        let sin_val: f64 = 30.0_f64.to_radians().sin();  // ~0.5
        let cos_val: f64 = 30.0_f64.to_radians().cos();  // ~0.866
        let _idx = doc.add_cone_surface(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [3.0, 0.0, 0.0],
            1.0,
            cos_val,
            sin_val,
        );

        let output = doc.to_sat_string();
        let doc2 = SatDocument::parse(&output).unwrap();
        let cones = doc2.records_of_type("cone-surface");
        let cone = SatConeSurface::from_record(cones[0]).unwrap();
        assert!((cone.sin_half_angle() - sin_val).abs() < 1e-10, "sin mismatch: {} vs {}", cone.sin_half_angle(), sin_val);
        assert!((cone.cos_half_angle() - cos_val).abs() < 1e-10, "cos mismatch: {} vs {}", cone.cos_half_angle(), cos_val);
    }
}
