//! Entity serialization for DWG object records.
//!
//! Each entity writer:
//! 1. Calls `write_common_entity_data()` (type code + preamble)
//! 2. Writes type-specific fields via the merged writer
//! 3. Calls `register_object()` (CRC, output, handle map)
//!
//! Ported from ACadSharp `DwgObjectWriter.Entities.cs`.

use crate::entities::*;
use crate::entities::raster_image::{ClipBoundary, ClipType};
use crate::io::dwg::dwg_reference_type::DwgReferenceType;
use crate::types::{Handle, Vector2, Vector3};

use super::common;
use super::DwgObjectWriter;

impl<'a> DwgObjectWriter<'a> {
    // â”€â”€ Entity dispatch â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Write a single entity record.
    pub(super) fn write_entity(&mut self, entity: &EntityType) {
        match entity {
            EntityType::Point(e) => self.write_point(e),
            EntityType::Line(e) => self.write_line(e),
            EntityType::Circle(e) => self.write_circle(e),
            EntityType::Arc(e) => self.write_arc(e),
            EntityType::Ellipse(e) => self.write_ellipse(e),
            EntityType::Text(e) => self.write_text(e),
            EntityType::MText(e) => self.write_mtext(e),
            EntityType::Solid(e) => self.write_solid(e),
            EntityType::Face3D(e) => self.write_face3d(e),
            EntityType::Insert(e) => self.write_insert(e),
            EntityType::LwPolyline(e) => self.write_lwpolyline(e),
            EntityType::Spline(e) => self.write_spline(e),
            EntityType::Ray(e) => self.write_ray(e),
            EntityType::XLine(e) => self.write_xline(e),
            EntityType::Leader(e) => self.write_leader(e),
            EntityType::Tolerance(e) => self.write_tolerance(e),
            EntityType::Shape(e) => self.write_shape(e),
            EntityType::Hatch(e) => self.write_hatch(e),
            EntityType::Viewport(e) => self.write_viewport_entity(e),
            EntityType::Dimension(e) => self.write_dimension(e),
            EntityType::Polyline2D(e) => self.write_polyline2d(e),
            EntityType::Polyline3D(e) => self.write_polyline3d(e),
            EntityType::PolyfaceMesh(e) => self.write_polyface_mesh(e),
            EntityType::PolygonMesh(e) => self.write_polygon_mesh(e),
            EntityType::Seqend(e) => self.write_seqend(e),
            EntityType::Mesh(e) => self.write_mesh(e),
            EntityType::MLine(e) => self.write_mline(e),
            EntityType::RasterImage(e) => self.write_raster_image(e),
            EntityType::Wipeout(e) => self.write_wipeout(e),
            EntityType::Ole2Frame(e) => self.write_ole2frame(e),
            EntityType::MultiLeader(e) => self.write_multileader(e),
            EntityType::AttributeDefinition(e) => self.write_attribute_definition(e),
            EntityType::AttributeEntity(e) => self.write_attribute_entity(e),
            EntityType::Polyline(e) => self.write_polyline_old(e),
            // Skip types that are structural or unsupported in DWG
            EntityType::Block(_) | EntityType::BlockEnd(_) => {}
            EntityType::Solid3D(e) => self.write_solid3d(e),
            EntityType::Region(e) => self.write_region(e),
            EntityType::Body(e) => self.write_body(e),
            EntityType::Table(_)
            | EntityType::Underlay(_) => {
                // Not yet supported â€” silently skip
            }
            EntityType::Unknown(e) => {
                // Write raw DWG data verbatim if available
                if let Some(ref raw_data) = e.raw_dwg_data {
                    self.register_raw_object(e.common.handle, raw_data, e.dwg_handle_bits);
                }
                // else: no raw data â†’ silently skip (DXF-only unknown)
            }
        }
    }

    // â”€â”€ Helper: write entity preamble â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn entity_preamble(&mut self, type_code: i16, c: &EntityCommon) {
        self.write_common_entity_data(
            type_code,
            c.handle,
            c.owner_handle,
            &c.layer,
            &c.color,
            &c.line_weight,
            &c.transparency,
            c.invisible,
            c.linetype_scale,
            &c.linetype,
            &c.extended_data,
            &c.reactors,
            &c.xdictionary_handle,
        );
    }

    // â”€â”€ Point â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_point(&mut self, e: &Point) {
        self.entity_preamble(common::OBJ_POINT, &e.common);
        self.writer.write_3bit_double(e.location);
        self.writer.write_bit_thickness(e.thickness);
        self.writer.write_bit_extrusion(e.normal);
        self.writer.write_bit_double(0.0); // x-axis angle
        self.register_object(e.common.handle);
    }

    // â”€â”€ Line â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_line(&mut self, e: &Line) {
        self.entity_preamble(common::OBJ_LINE, &e.common);

        if self.version.r13_14_only() {
            self.writer.write_3bit_double(e.start);
            self.writer.write_3bit_double(e.end);
        } else {
            // R2000+: z-are-zero optimization
            let z_are_zero = e.start.z == 0.0 && e.end.z == 0.0;
            self.writer.write_bit(z_are_zero);
            self.writer.write_raw_double(e.start.x);
            self.writer
                .write_bit_double_with_default(e.end.x, e.start.x);
            self.writer.write_raw_double(e.start.y);
            self.writer
                .write_bit_double_with_default(e.end.y, e.start.y);
            if !z_are_zero {
                self.writer.write_raw_double(e.start.z);
                self.writer
                    .write_bit_double_with_default(e.end.z, e.start.z);
            }
        }

        self.writer.write_bit_thickness(e.thickness);
        self.writer.write_bit_extrusion(e.normal);

        self.register_object(e.common.handle);
    }

    // â”€â”€ Circle â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_circle(&mut self, e: &Circle) {
        self.entity_preamble(common::OBJ_CIRCLE, &e.common);
        self.writer.write_3bit_double(e.center);
        self.writer.write_bit_double(e.radius);
        self.writer.write_bit_thickness(e.thickness);
        self.writer.write_bit_extrusion(e.normal);
        self.register_object(e.common.handle);
    }

    // â”€â”€ Arc â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_arc(&mut self, e: &Arc) {
        self.entity_preamble(common::OBJ_ARC, &e.common);
        self.writer.write_3bit_double(e.center);
        self.writer.write_bit_double(e.radius);
        self.writer.write_bit_thickness(e.thickness);
        self.writer.write_bit_extrusion(e.normal);
        self.writer.write_bit_double(e.start_angle);
        self.writer.write_bit_double(e.end_angle);
        self.register_object(e.common.handle);
    }

    // â”€â”€ Ellipse â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_ellipse(&mut self, e: &Ellipse) {
        self.entity_preamble(common::OBJ_ELLIPSE, &e.common);
        self.writer.write_3bit_double(e.center);
        self.writer.write_3bit_double(e.major_axis);
        self.writer.write_3bit_double(e.normal);
        self.writer.write_bit_double(e.minor_axis_ratio);
        self.writer.write_bit_double(e.start_parameter);
        self.writer.write_bit_double(e.end_parameter);
        self.register_object(e.common.handle);
    }

    // â”€â”€ Text â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_text(&mut self, e: &Text) {
        self.entity_preamble(common::OBJ_TEXT, &e.common);

        let alignment_point = e.alignment_point.unwrap_or(Vector3::ZERO);

        if self.version.r13_14_only() {
            // Elevation BD
            self.writer.write_bit_double(e.insertion_point.z);
            // Insertion pt 2RD 10
            self.writer.write_raw_double(e.insertion_point.x);
            self.writer.write_raw_double(e.insertion_point.y);
            // Alignment pt 2RD 11
            self.writer.write_raw_double(alignment_point.x);
            self.writer.write_raw_double(alignment_point.y);
            // Extrusion 3BD 210
            self.writer.write_3bit_double(e.normal);
            // Thickness BD 39
            self.writer.write_bit_double(0.0);
            // Oblique ang BD 51
            self.writer.write_bit_double(e.oblique_angle);
            // Rotation ang BD 50
            self.writer.write_bit_double(e.rotation);
            // Height BD 40
            self.writer.write_bit_double(e.height);
            // Width factor BD 41
            self.writer.write_bit_double(e.width_factor);
            // Text value TV 1
            self.writer.write_variable_text(&e.value);
            // Generation BS 71
            self.writer.write_bit_short(0); // mirror = None
            // Horiz align BS 72
            self.writer.write_bit_short(e.horizontal_alignment as i16);
            // Vert align BS 73
            self.writer.write_bit_short(e.vertical_alignment as i16);
        } else {
            // R2000+: DataFlags RC â€” presence bits for subsequent data
            let mut data_flags: u8 = 0;
            // 0x01 = elevation (InsertPoint.Z) is 0
            if e.insertion_point.z == 0.0 {
                data_flags |= 0x01;
            }
            // 0x02 = alignment point is zero
            if alignment_point.x == 0.0
                && alignment_point.y == 0.0
                && alignment_point.z == 0.0
            {
                data_flags |= 0x02;
            }
            // 0x04 = oblique angle is 0
            if e.oblique_angle == 0.0 {
                data_flags |= 0x04;
            }
            // 0x08 = rotation is 0
            if e.rotation == 0.0 {
                data_flags |= 0x08;
            }
            // 0x10 = width factor is 1.0
            if e.width_factor == 1.0 {
                data_flags |= 0x10;
            }
            // 0x20 = mirror flag is None (0)
            data_flags |= 0x20; // always None, no mirror field in struct
            // 0x40 = horizontal alignment is Left (0)
            if e.horizontal_alignment as u8 == 0 {
                data_flags |= 0x40;
            }
            // 0x80 = vertical alignment is Baseline (0)
            if e.vertical_alignment as u8 == 0 {
                data_flags |= 0x80;
            }
            self.writer.write_byte(data_flags);

            // Elevation RD â€” present if !(DataFlags & 0x01)
            if (data_flags & 0x01) == 0 {
                self.writer.write_raw_double(e.insertion_point.z);
            }
            // Insertion pt 2RD 10
            self.writer.write_raw_double(e.insertion_point.x);
            self.writer.write_raw_double(e.insertion_point.y);
            // Alignment pt 2DD 11 â€” present if !(DataFlags & 0x02)
            // Uses insertion pt X,Y as default values
            if (data_flags & 0x02) == 0 {
                self.writer
                    .write_bit_double_with_default(alignment_point.x, e.insertion_point.x);
                self.writer
                    .write_bit_double_with_default(alignment_point.y, e.insertion_point.y);
            }
            // Extrusion BE 210
            self.writer.write_bit_extrusion(e.normal);
            // Thickness BT 39
            self.writer.write_bit_thickness(0.0);
            // Oblique ang RD 51 â€” present if !(DataFlags & 0x04)
            if (data_flags & 0x04) == 0 {
                self.writer.write_raw_double(e.oblique_angle);
            }
            // Rotation ang RD 50 â€” present if !(DataFlags & 0x08)
            if (data_flags & 0x08) == 0 {
                self.writer.write_raw_double(e.rotation);
            }
            // Height RD 40 (always present)
            self.writer.write_raw_double(e.height);
            // Width factor RD 41 â€” present if !(DataFlags & 0x10)
            if (data_flags & 0x10) == 0 {
                self.writer.write_raw_double(e.width_factor);
            }
            // Text value TV 1
            self.writer.write_variable_text(&e.value);
            // Generation BS 71 â€” present if !(DataFlags & 0x20)
            if (data_flags & 0x20) == 0 {
                self.writer.write_bit_short(0); // mirror = None
            }
            // Horiz align BS 72 â€” present if !(DataFlags & 0x40)
            if (data_flags & 0x40) == 0 {
                self.writer
                    .write_bit_short(e.horizontal_alignment as i16);
            }
            // Vert align BS 73 â€” present if !(DataFlags & 0x80)
            if (data_flags & 0x80) == 0 {
                self.writer
                    .write_bit_short(e.vertical_alignment as i16);
            }
        }

        // Style handle
        let style_handle = self
            .document
            .text_styles
            .get(&e.style)
            .map(|s| s.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, style_handle.value());

        self.register_object(e.common.handle);
    }

    // â”€â”€ MText â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_mtext(&mut self, e: &MText) {
        self.entity_preamble(common::OBJ_MTEXT, &e.common);

        // Insertion pt 3BD 10
        self.writer.write_3bit_double(e.insertion_point);
        // Extrusion 3BD 210 (NOT BitExtrusion â€” full 3BD per spec)
        self.writer.write_3bit_double(e.normal);

        // X-axis dir 3BD 11 (alignment point / direction vector)
        let x_dir = Vector3::new(e.rotation.cos(), e.rotation.sin(), 0.0);
        self.writer.write_3bit_double(x_dir);

        // Rect width BD 41
        self.writer.write_bit_double(e.rectangle_width);

        // R2007+: Rect height BD 46
        if self.version.r2007_plus() {
            self.writer
                .write_bit_double(e.rectangle_height.unwrap_or(0.0));
        }

        // Text height BD 40
        self.writer.write_bit_double(e.height);
        // Attachment BS 71
        self.writer.write_bit_short(e.attachment_point as i16);
        // Drawing dir BS 72 (unconditional â€” written for ALL versions)
        self.writer.write_bit_short(e.drawing_direction as i16);

        // Extents ht BD (undocumented, not in DXF)
        self.writer.write_bit_double(0.0);
        // Extents wid BD (undocumented, not in DXF)
        self.writer.write_bit_double(0.0);

        // Text TV 1
        self.writer.write_variable_text(&e.value);

        // H 7 STYLE (hard pointer) â€” written BEFORE R2000+ block
        let style_handle = self
            .document
            .text_styles
            .get(&e.style)
            .map(|s| s.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, style_handle.value());

        // Linespacing Style BS 73 (1=At Least, 2=Exact)
        self.writer.write_bit_short(1);
        // Linespacing Factor BD 44
        self.writer.write_bit_double(e.line_spacing_factor);
        // Unknown bit B
        self.writer.write_bit(false);

        // R2004+:
        if self.version.r2004_plus() {
            // Background flags BL 90 (0 = no background)
            self.writer.write_bit_long(0);
        }

        // R2018+:
        if self.version.r2018_plus(self.dxf_version) {
            // Is NOT annotative B
            // Write false = "it IS annotative" = skip redundant fields
            self.writer.write_bit(false);
        }

        self.register_object(e.common.handle);
    }

    // â”€â”€ Solid â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_solid(&mut self, e: &Solid) {
        self.entity_preamble(common::OBJ_SOLID, &e.common);
        self.writer.write_bit_thickness(e.thickness);
        self.writer.write_bit_double(e.first_corner.z);
        self.writer
            .write_2raw_double(Vector2::new(e.first_corner.x, e.first_corner.y));
        self.writer
            .write_2raw_double(Vector2::new(e.second_corner.x, e.second_corner.y));
        self.writer
            .write_2raw_double(Vector2::new(e.third_corner.x, e.third_corner.y));
        self.writer
            .write_2raw_double(Vector2::new(e.fourth_corner.x, e.fourth_corner.y));
        self.writer.write_bit_extrusion(e.normal);
        self.register_object(e.common.handle);
    }

    // â”€â”€ Face3D â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_face3d(&mut self, e: &Face3D) {
        self.entity_preamble(common::OBJ_3DFACE, &e.common);

        if self.version.r13_14_only() {
            self.writer.write_3bit_double(e.first_corner);
            self.writer.write_3bit_double(e.second_corner);
            self.writer.write_3bit_double(e.third_corner);
            self.writer.write_3bit_double(e.fourth_corner);
            self.writer
                .write_bit_short(e.invisible_edges.bits() as i16);
        } else {
            // R2000+
            let has_no_flags = e.invisible_edges.bits() == 0;
            self.writer.write_bit(has_no_flags);

            let z_is_zero = e.first_corner.z == 0.0
                && e.second_corner.z == 0.0
                && e.third_corner.z == 0.0
                && e.fourth_corner.z == 0.0;
            self.writer.write_bit(z_is_zero);

            self.writer.write_raw_double(e.first_corner.x);
            self.writer.write_raw_double(e.first_corner.y);
            if !z_is_zero {
                self.writer.write_raw_double(e.first_corner.z);
            }

            // 2nd corner 3DD (default = 1st corner)
            self.writer
                .write_bit_double_with_default(e.second_corner.x, e.first_corner.x);
            self.writer
                .write_bit_double_with_default(e.second_corner.y, e.first_corner.y);
            if !z_is_zero {
                self.writer
                    .write_bit_double_with_default(e.second_corner.z, e.first_corner.z);
            }

            // 3rd corner 3DD (default = 2nd corner)
            self.writer
                .write_bit_double_with_default(e.third_corner.x, e.second_corner.x);
            self.writer
                .write_bit_double_with_default(e.third_corner.y, e.second_corner.y);
            if !z_is_zero {
                self.writer
                    .write_bit_double_with_default(e.third_corner.z, e.second_corner.z);
            }

            // 4th corner 3DD (default = 3rd corner)
            self.writer
                .write_bit_double_with_default(e.fourth_corner.x, e.third_corner.x);
            self.writer
                .write_bit_double_with_default(e.fourth_corner.y, e.third_corner.y);
            if !z_is_zero {
                self.writer
                    .write_bit_double_with_default(e.fourth_corner.z, e.third_corner.z);
            }

            if !has_no_flags {
                self.writer
                    .write_bit_short(e.invisible_edges.bits() as i16);
            }
        }

        self.register_object(e.common.handle);
    }

    // â”€â”€ Insert â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_insert(&mut self, e: &Insert) {
        self.entity_preamble(common::OBJ_INSERT, &e.common);

        // Ins pt 3BD 10
        self.writer.write_3bit_double(e.insert_point);

        if self.version.r13_14_only() {
            // R13-R14: X/Y/Z Scale as separate BD values
            self.writer.write_bit_double(e.x_scale());
            self.writer.write_bit_double(e.y_scale());
            self.writer.write_bit_double(e.z_scale());
        }

        if self.version.r2000_plus() {
            // R2000+: Data flags BB + conditional scale data
            let sx = e.x_scale();
            let sy = e.y_scale();
            let sz = e.z_scale();

            if sx == 1.0 && sy == 1.0 && sz == 1.0 {
                // 11 - scale is (1.0, 1.0, 1.0), no data stored
                self.writer.write_2bits(3);
            } else if sx == sy && sx == sz {
                // 10 - 41 value stored as RD, 42 & 43 assumed equal to 41
                self.writer.write_2bits(2);
                self.writer.write_raw_double(sx);
            } else if sx == 1.0 {
                // 01 - 41 is 1.0, 2 DD's present using 1.0 as default
                self.writer.write_2bits(1);
                self.writer.write_bit_double_with_default(sy, 1.0);
                self.writer.write_bit_double_with_default(sz, 1.0);
            } else {
                // 00 - 41 as RD, then 42 as DD (default=41), 43 as DD (default=41)
                self.writer.write_2bits(0);
                self.writer.write_raw_double(sx);
                self.writer.write_bit_double_with_default(sy, sx);
                self.writer.write_bit_double_with_default(sz, sx);
            }
        }

        // Rotation BD 50
        self.writer.write_bit_double(e.rotation);
        // Extrusion 3BD 210
        self.writer.write_3bit_double(e.normal);
        // Has ATTRIBs B 66
        self.writer.write_bit(e.has_attributes());

        // R2004+: owned object count when has_attribs
        let (attrib_handles, seqend_handle) = if e.has_attributes() {
            let ahs: Vec<Handle> = (0..e.attributes.len())
                .map(|_| self.alloc_handle())
                .collect();
            let sh = self.alloc_handle();

            if self.version.r2004_plus() {
                // owned_object_count = attribs (SEQEND written separately)
                self.writer
                    .write_bit_long(e.attributes.len() as i32);
            }
            (ahs, sh)
        } else {
            (Vec::new(), Handle::NULL)
        };

        // Block header ref (hard pointer)
        let block_handle = self
            .document
            .block_records
            .get(&e.block_name)
            .map(|br| br.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, block_handle.value());

        // Attribute owned handles (if present)
        if e.has_attributes() {
            if self.version.r13_15_only() {
                // R13-R2000: first attrib, last attrib
                let first = attrib_handles.first().copied().unwrap_or(Handle::NULL);
                let last = attrib_handles.last().copied().unwrap_or(Handle::NULL);
                self.writer
                    .write_handle(DwgReferenceType::SoftPointer, first.value());
                self.writer
                    .write_handle(DwgReferenceType::SoftPointer, last.value());
            } else if self.version.r2004_plus() {
                for &ah in &attrib_handles {
                    self.writer
                        .write_handle(DwgReferenceType::HardOwnership, ah.value());
                }
            }
            // SEQEND handle
            self.writer
                .write_handle(DwgReferenceType::HardOwnership, seqend_handle.value());
        }

        self.register_object(e.common.handle);

        // Write child ATTRIB entities + SEQEND
        if e.has_attributes() {
            let saved_prev = self.prev_handle.take();
            let saved_next = self.next_handle.take();

            let sub_count = attrib_handles.len() + 1; // attribs + seqend
            for (i, (att, &ah)) in e.attributes.iter().zip(attrib_handles.iter()).enumerate() {
                self.prev_handle = if i > 0 {
                    Some(attrib_handles[i - 1])
                } else {
                    None
                };
                self.next_handle = if i + 1 < sub_count {
                    if i + 1 < attrib_handles.len() {
                        Some(attrib_handles[i + 1])
                    } else {
                        Some(seqend_handle)
                    }
                } else {
                    None
                };
                self.write_attribute_entity_child(att, ah, e.common.handle);
            }

            // Write SEQEND
            self.prev_handle = attrib_handles.last().copied();
            self.next_handle = None;
            self.write_common_entity_data(
                common::OBJ_SEQEND,
                seqend_handle,
                e.common.handle,
                &e.common.layer,
                &e.common.color,
                &crate::types::LineWeight::ByLayer,
                &crate::types::Transparency::default(),
                false,
                1.0,
                "ByLayer",
                &crate::xdata::ExtendedData::default(),
                &[],
                &None,
            );
            self.register_object(seqend_handle);

            self.prev_handle = saved_prev;
            self.next_handle = saved_next;
        }
    }

    /// Write a child ATTRIB entity owned by an INSERT.
    fn write_attribute_entity_child(
        &mut self,
        att: &AttributeEntity,
        handle: Handle,
        owner: Handle,
    ) {
        self.write_common_entity_data(
            common::OBJ_ATTRIB,
            handle,
            owner,
            &att.common.layer,
            &att.common.color,
            &att.common.line_weight,
            &att.common.transparency,
            att.common.invisible,
            att.common.linetype_scale,
            &att.common.linetype,
            &att.common.extended_data,
            &att.common.reactors,
            &att.common.xdictionary_handle,
        );

        // writeTextEntity portion
        self.write_text_entity_data(
            att.insertion_point,
            att.alignment_point,
            att.normal,
            0.0, // thickness
            att.oblique_angle,
            att.rotation,
            att.height,
            att.width_factor,
            &att.value,
            0,  // generation (text mirror flags)
            att.horizontal_alignment as i16,
            att.vertical_alignment as i16,
        );

        // Style handle
        let style_handle = self
            .document
            .text_styles
            .get(&att.text_style)
            .map(|s| s.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, style_handle.value());

        // writeCommonAttData: R2010+ version byte
        if self.version.r2010_plus() {
            self.writer.write_byte(0);
        }

        // R2018+: AttributeType byte
        if self.version.r2018_plus(self.dxf_version) {
            self.writer.write_byte(1); // SingleLine
        }

        // Tag, field length, flags
        self.writer.write_variable_text(&att.tag);
        self.writer.write_bit_short(att.field_length);
        let flag_byte = att.flags.to_bits();
        self.writer.write_byte(flag_byte as u8);

        // R2007+: lock position
        if self.version.r2007_plus() {
            self.writer.write_bit(false);
        }

        self.register_object(handle);
    }

    // â”€â”€ LwPolyline â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_lwpolyline(&mut self, e: &LwPolyline) {
        self.entity_preamble(common::OBJ_LWPOLYLINE, &e.common);

        let num_pts = e.vertices.len() as i32;

        // Check for presence of optional data
        let has_widths = e.vertices.iter().any(|v| v.start_width != 0.0 || v.end_width != 0.0);
        let has_bulges = e.vertices.iter().any(|v| v.bulge != 0.0);
        let has_constant_width = e.constant_width != 0.0;
        let has_elevation = e.elevation != 0.0;
        let has_thickness = e.thickness != 0.0;
        let has_normal = e.normal != Vector3::UNIT_Z;

        // Build flags
        let mut flag: i16 = 0;
        if has_normal      { flag |= 0x1; }
        if has_thickness    { flag |= 0x2; }
        if has_constant_width { flag |= 0x4; }
        if has_elevation    { flag |= 0x8; }
        if has_bulges       { flag |= 0x10; }
        if has_widths       { flag |= 0x20; }
        if e.plinegen       { flag |= 0x100; }
        if e.is_closed      { flag |= 0x200; }

        self.writer.write_bit_short(flag);

        if has_constant_width {
            self.writer.write_bit_double(e.constant_width);
        }
        if has_elevation {
            self.writer.write_bit_double(e.elevation);
        }
        // BT / BE types: self-compressing, but still flag-gated
        if has_thickness {
            self.writer.write_bit_thickness(e.thickness);
        }
        if has_normal {
            self.writer.write_bit_extrusion(e.normal);
        }

        self.writer.write_bit_long(num_pts);

        if has_bulges {
            self.writer.write_bit_long(num_pts);
        }
        if has_widths {
            self.writer.write_bit_long(num_pts);
        }

        // R13-R14: simple 2RD for each vertex
        if self.version.r13_14_only() {
            for v in &e.vertices {
                self.writer.write_raw_double(v.location.x);
                self.writer.write_raw_double(v.location.y);
            }
        }

        // R2000+: first vertex is 2RD, rest are 2DD with previous as default
        if self.version.r2000_plus() && !e.vertices.is_empty() {
            let first = &e.vertices[0];
            self.writer.write_raw_double(first.location.x);
            self.writer.write_raw_double(first.location.y);
            
            for i in 1..e.vertices.len() {
                let curr = &e.vertices[i];
                let prev = &e.vertices[i - 1];
                self.writer.write_bit_double_with_default(curr.location.x, prev.location.x);
                self.writer.write_bit_double_with_default(curr.location.y, prev.location.y);
            }
        }

        // Bulges
        if has_bulges {
            for v in &e.vertices {
                self.writer.write_bit_double(v.bulge);
            }
        }

        // Widths
        if has_widths {
            for v in &e.vertices {
                self.writer.write_bit_double(v.start_width);
                self.writer.write_bit_double(v.end_width);
            }
        }

        self.register_object(e.common.handle);
    }

    // â”€â”€ Ray â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_ray(&mut self, e: &Ray) {
        self.entity_preamble(common::OBJ_RAY, &e.common);
        self.writer.write_3bit_double(e.base_point);
        self.writer.write_3bit_double(e.direction);
        self.register_object(e.common.handle);
    }

    // â”€â”€ XLine â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_xline(&mut self, e: &XLine) {
        self.entity_preamble(common::OBJ_XLINE, &e.common);
        self.writer.write_3bit_double(e.base_point);
        self.writer.write_3bit_double(e.direction);
        self.register_object(e.common.handle);
    }

    // â”€â”€ Spline â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_spline(&mut self, e: &Spline) {
        self.entity_preamble(common::OBJ_SPLINE, &e.common);

        // Determine scenario: 2 = fit points, 1 = control points/knots
        let scenario: i32 = if !e.fit_points.is_empty() { 2 } else { 1 };

        if self.version.r2013_plus(self.dxf_version) {
            // R2013+: scenario BL, flags1 BL, knot parametrization BL
            self.writer.write_bit_long(scenario);
            self.writer.write_bit_long(0); // flags1
            self.writer.write_bit_long(0); // knot parametrization
        } else {
            // Scenario BL
            self.writer.write_bit_long(scenario);
        }

        // Degree BL (common, before scenario switch)
        self.writer.write_bit_long(e.degree);

        let has_weights = !e.weights.is_empty();

        match scenario {
            1 => {
                // Scenario 1: control-point spline
                // Rational B (flag bit 2)
                self.writer.write_bit(e.flags.rational);
                // Closed B (flag bit 0)
                self.writer.write_bit(e.flags.closed);
                // Periodic B (flag bit 1)
                self.writer.write_bit(e.flags.periodic);

                // Knot tol BD 42
                self.writer.write_bit_double(1e-10);
                // Ctrl tol BD 43
                self.writer.write_bit_double(1e-10);

                // Generate clamped uniform knot vector if not provided
                let knots: Vec<f64> = if e.knots.is_empty() && !e.control_points.is_empty() {
                    Spline::generate_clamped_knots(e.degree as usize, e.control_points.len())
                } else {
                    e.knots.clone()
                };

                // Numknots BL 72
                self.writer.write_bit_long(knots.len() as i32);
                // Numctrlpts BL 73
                self.writer.write_bit_long(e.control_points.len() as i32);

                // Weight B (echo of rational flag for weights present)
                self.writer.write_bit(has_weights);

                // Knots
                for k in &knots {
                    self.writer.write_bit_double(*k);
                }

                // Control points + weights
                for (i, pt) in e.control_points.iter().enumerate() {
                    self.writer.write_3bit_double(*pt);
                    if has_weights {
                        let w = e.weights.get(i).copied().unwrap_or(1.0);
                        self.writer.write_bit_double(w);
                    }
                }
            }
            _ => {
                // Scenario 2: fit-point spline
                // Fit Tol BD 44
                self.writer.write_bit_double(0.0);
                // Beg tan vec 3BD 12
                self.writer.write_3bit_double(Vector3::ZERO);
                // End tan vec 3BD 13
                self.writer.write_3bit_double(Vector3::ZERO);
                // num fit pts BL 74
                self.writer.write_bit_long(e.fit_points.len() as i32);
                // Fit points
                for pt in &e.fit_points {
                    self.writer.write_3bit_double(*pt);
                }
            }
        }

        self.register_object(e.common.handle);
    }

    // â”€â”€ Leader â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_leader(&mut self, e: &Leader) {
        self.entity_preamble(common::OBJ_LEADER, &e.common);

        // Unknown B
        self.writer.write_bit(false);
        // Annotation type BS
        self.writer.write_bit_short(e.creation_type.to_value());
        // Path type BS
        self.writer.write_bit_short(e.path_type as i16);

        // Numpts BL + vertices
        self.writer.write_bit_long(e.vertices.len() as i32);
        for pt in &e.vertices {
            self.writer.write_3bit_double(*pt);
        }

        // Origin 3BD (first vertex by default)
        let origin = e.vertices.first().copied().unwrap_or(Vector3::ZERO);
        self.writer.write_3bit_double(origin);
        // Extrusion 3BD 210
        self.writer.write_3bit_double(e.normal);
        // X direction 3BD 211
        self.writer.write_3bit_double(e.horizontal_direction);
        // Offsettoblockinspt 3BD 212
        self.writer.write_3bit_double(e.block_offset);

        // R14+: Endptproj 3BD (annotation offset) â€” not present in R13
        if self.dxf_version >= crate::types::DxfVersion::AC1014 {
            self.writer.write_3bit_double(e.annotation_offset);
        }

        // R13-R14 Only: DIMGAP and arrowhead data
        if self.version.r13_14_only() {
            self.writer.write_bit_double(0.0); // DIMGAP * DIMSCALE
        }

        // Common: text height / width (â‰¤ R2007)
        if !self.version.r2010_plus() {
            self.writer.write_bit_double(e.text_height);
            self.writer.write_bit_double(e.text_width);
        }

        // Hooklineonxdir B
        self.writer.write_bit(e.hookline_direction == HooklineDirection::Same);
        // Arrowheadon B
        self.writer.write_bit(e.arrow_enabled);

        // R13-R14 Only: arrowhead block
        if self.version.r13_14_only() {
            self.writer.write_bit_short(0); // arrowhead type
            self.writer.write_bit_double(0.0); // dimasz
            self.writer.write_bit(false); // unknown
            self.writer.write_bit(false); // unknown
            self.writer.write_bit_short(0); // unknown BS
            self.writer.write_bit_short(0); // byblockcolor BS
            self.writer.write_bit(false); // unknown
            self.writer.write_bit(false); // unknown
        }

        // R2000+:
        if self.version.r2000_plus() {
            self.writer.write_bit_short(0); // unknown BS
            self.writer.write_bit(false); // unknown B
            self.writer.write_bit(false); // unknown B
        }

        // H 340 Associated annotation (hard pointer, null)
        self.writer
            .write_handle(DwgReferenceType::HardPointer, 0);

        // H 2 DIMSTYLE (hard pointer)
        let dimstyle_handle = self
            .document
            .dim_styles
            .get(&e.dimension_style)
            .map(|d| d.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, dimstyle_handle.value());

        self.register_object(e.common.handle);
    }

    // â”€â”€ Tolerance â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_tolerance(&mut self, e: &Tolerance) {
        self.entity_preamble(common::OBJ_TOLERANCE, &e.common);

        // R13-R14 Only:
        if self.version.r13_14_only() {
            self.writer.write_bit_short(0); // unknown short
            self.writer.write_bit_double(e.text_height); // Height BD
            self.writer.write_bit_double(e.dimension_gap); // Dimgap BD
        }

        // Common:
        // Ins pt 3BD 10
        self.writer.write_3bit_double(e.insertion_point);
        // X direction 3BD 11
        self.writer.write_3bit_double(e.direction);
        // Extrusion 3BD 210
        self.writer.write_3bit_double(e.normal);
        // Text string BS 1
        self.writer.write_variable_text(&e.text);

        // Dim style handle (hard pointer)
        let ds_handle = e
            .dimension_style_handle
            .unwrap_or(
                self.document
                    .dim_styles
                    .get(&e.dimension_style_name)
                    .map(|d| d.handle)
                    .unwrap_or(Handle::NULL),
            );
        self.writer
            .write_handle(DwgReferenceType::HardPointer, ds_handle.value());

        self.register_object(e.common.handle);
    }

    // â”€â”€ Shape â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_shape(&mut self, e: &Shape) {
        self.entity_preamble(common::OBJ_SHAPE, &e.common);

        // Ins pt 3BD 10
        self.writer.write_3bit_double(e.insertion_point);
        // Size BD 40
        self.writer.write_bit_double(e.size);
        // Rotation BD 50
        self.writer.write_bit_double(e.rotation);
        // Relative X Scale BD 41
        self.writer.write_bit_double(e.relative_x_scale);
        // Oblique angle BD 51
        self.writer.write_bit_double(e.oblique_angle);
        // Thickness BD 39
        self.writer.write_bit_double(e.thickness);
        // Shape index BS 2
        self.writer.write_bit_short(e.shape_number as i16);
        // Extrusion 3BD 210
        self.writer.write_3bit_double(e.normal);

        // SHAPEFILE style handle (hard pointer)
        let sh = e.style_handle.unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, sh.value());

        self.register_object(e.common.handle);
    }

    // â”€â”€ Hatch â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_hatch(&mut self, e: &Hatch) {
        self.entity_preamble(common::OBJ_HATCH, &e.common);

        // Gradient color data (R2004+)
        if self.version.r2004_plus() {
            let is_gradient = e.gradient_color.enabled;
            self.writer.write_bit_long(if is_gradient { 1 } else { 0 });

            // All gradient fields must be written unconditionally
            self.writer
                .write_bit_long(e.gradient_color.reserved);
            self.writer.write_bit_double(e.gradient_color.angle);
            self.writer.write_bit_double(e.gradient_color.shift);
            self.writer
                .write_bit_long(if e.gradient_color.is_single_color {
                    1
                } else {
                    0
                });
            self.writer.write_bit_double(0.0); // color tint

            self.writer
                .write_bit_long(e.gradient_color.colors.len() as i32);
            for entry in &e.gradient_color.colors {
                self.writer.write_bit_double(entry.value);
                self.writer.write_cm_color(&entry.color);
            }

            self.writer.write_variable_text(&e.gradient_color.name);
        }

        // Elevation (Z of insertion point)
        self.writer.write_bit_double(e.elevation);
        self.writer.write_3bit_double(e.normal);
        self.writer.write_variable_text(&e.pattern.name);

        // Solid fill flag
        self.writer.write_bit(e.is_solid);
        // Associative flag
        self.writer.write_bit(e.is_associative);

        // Boundary paths
        let mut has_derived_boundary = false;
        self.writer.write_bit_long(e.paths.len() as i32);
        for path in &e.paths {
            if path.flags.is_derived() {
                has_derived_boundary = true;
            }
            self.write_hatch_boundary_path(path);
        }

        // Hatch style
        self.writer.write_bit_short(e.style as i16);
        // Pattern type
        self.writer.write_bit_short(e.pattern_type as i16);

        if !e.is_solid {
            // Pattern angle + scale + double flag
            self.writer.write_bit_double(e.pattern_angle);
            self.writer.write_bit_double(e.pattern_scale);
            self.writer.write_bit(e.is_double);

            // Pattern definition lines
            self.writer
                .write_bit_short(e.pattern.lines.len() as i16);
            for line in &e.pattern.lines {
                self.writer.write_bit_double(line.angle);
                self.writer
                    .write_2bit_double(line.base_point);
                self.writer
                    .write_2bit_double(line.offset);
                self.writer
                    .write_bit_short(line.dash_lengths.len() as i16);
                for d in &line.dash_lengths {
                    self.writer.write_bit_double(*d);
                }
            }
        }

        // Pixel size â€” only written when a Derived boundary path exists
        if has_derived_boundary {
            self.writer.write_bit_double(e.pixel_size);
        }

        // Seed points
        self.writer.write_bit_long(e.seed_points.len() as i32);
        for sp in &e.seed_points {
            self.writer
                .write_2raw_double(*sp);
        }

        // Boundary object handles
        for path in &e.paths {
            for h in &path.boundary_handles {
                self.writer
                    .write_handle(DwgReferenceType::SoftPointer, h.value());
            }
        }

        self.register_object(e.common.handle);
    }

    fn write_hatch_boundary_path(&mut self, path: &BoundaryPath) {
        self.writer.write_bit_long(path.flags.bits() as i32);

        let is_polyline = (path.flags.bits() & 2) != 0;

        if !is_polyline {
            // Edges
            self.writer.write_bit_long(path.edges.len() as i32);
            for edge in &path.edges {
                match edge {
                    BoundaryEdge::Line(le) => {
                        self.writer.write_byte(1);
                        self.writer
                            .write_2raw_double(le.start);
                        self.writer
                            .write_2raw_double(le.end);
                    }
                    BoundaryEdge::CircularArc(ca) => {
                        self.writer.write_byte(2);
                        self.writer
                            .write_2raw_double(ca.center);
                        self.writer.write_bit_double(ca.radius);
                        self.writer.write_bit_double(ca.start_angle);
                        self.writer.write_bit_double(ca.end_angle);
                        self.writer.write_bit(ca.counter_clockwise);
                    }
                    BoundaryEdge::EllipticArc(ea) => {
                        self.writer.write_byte(3);
                        self.writer
                            .write_2raw_double(ea.center);
                        self.writer
                            .write_2raw_double(ea.major_axis_endpoint);
                        self.writer
                            .write_bit_double(ea.minor_axis_ratio);
                        self.writer.write_bit_double(ea.start_angle);
                        self.writer.write_bit_double(ea.end_angle);
                        self.writer.write_bit(ea.counter_clockwise);
                    }
                    BoundaryEdge::Spline(se) => {
                        self.writer.write_byte(4);
                        self.writer.write_bit_long(se.degree as i32);
                        self.writer.write_bit(se.rational);
                        self.writer.write_bit(se.periodic);

                        self.writer
                            .write_bit_long(se.knots.len() as i32);
                        self.writer
                            .write_bit_long(se.control_points.len() as i32);
                        for k in &se.knots {
                            self.writer.write_bit_double(*k);
                        }
                        for pt in &se.control_points {
                            // Control points are 2D in hatch boundary splines
                            self.writer
                                .write_2raw_double(Vector2::new(pt.x, pt.y));
                            if se.rational {
                                // Weight stored in Z
                                self.writer.write_bit_double(pt.z);
                            }
                        }

                        // Fit data â€” R2010+ only
                        if self.version.r2010_plus() {
                            self.writer
                                .write_bit_long(se.fit_points.len() as i32);
                            if !se.fit_points.is_empty() {
                                for pt in &se.fit_points {
                                    self.writer
                                        .write_2raw_double(*pt);
                                }

                                self.writer
                                    .write_2raw_double(se.start_tangent);
                                self.writer
                                    .write_2raw_double(se.end_tangent);
                            }
                        }
                    }
                    BoundaryEdge::Polyline(pe) => {
                        // Polyline edges should use polyline flag path
                        self.writer.write_byte(1);
                        // Simplified: write as line segments
                        for (i, _v) in pe.vertices.iter().enumerate() {
                            if i + 1 < pe.vertices.len() {
                                let s = pe.vertices[i];
                                let e = pe.vertices[i + 1];
                                self.writer
                                    .write_2raw_double(Vector2::new(s.x, s.y));
                                self.writer
                                    .write_2raw_double(Vector2::new(e.x, e.y));
                            }
                        }
                    }
                }
            }
        } else {
            // Polyline boundary path
            // Find the polyline edge
            if let Some(BoundaryEdge::Polyline(pe)) = path.edges.first() {
                let has_bulge = pe
                    .vertices
                    .iter()
                    .any(|v| v.z != 0.0); // z stores bulge
                self.writer.write_bit(has_bulge);
                self.writer.write_bit(pe.is_closed);
                self.writer
                    .write_bit_long(pe.vertices.len() as i32);
                for v in &pe.vertices {
                    self.writer
                        .write_2raw_double(Vector2::new(v.x, v.y));
                    if has_bulge {
                        self.writer.write_bit_double(v.z); // bulge
                    }
                }
            }
        }

        // Boundary object count
        self.writer
            .write_bit_long(path.boundary_handles.len() as i32);
    }

    // â”€â”€ Viewport entity â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_viewport_entity(&mut self, e: &Viewport) {
        self.entity_preamble(common::OBJ_VIEWPORT, &e.common);

        // Center 3BD 10
        self.writer.write_3bit_double(e.center);
        // Width BD 40
        self.writer.write_bit_double(e.width);
        // Height BD 41
        self.writer.write_bit_double(e.height);

        // View data (written for all versions)
        self.writer.write_3bit_double(e.view_target);
        self.writer.write_3bit_double(e.view_direction);
        self.writer.write_bit_double(e.twist_angle);
        self.writer.write_bit_double(e.view_height);
        self.writer.write_bit_double(e.lens_length);
        self.writer.write_bit_double(e.front_clip_z);
        self.writer.write_bit_double(e.back_clip_z);
        self.writer.write_bit_double(e.snap_angle);
        self.writer
            .write_2raw_double(Vector2::new(e.view_center.x, e.view_center.y));
        self.writer
            .write_2raw_double(Vector2::new(e.snap_base.x, e.snap_base.y));
        self.writer
            .write_2raw_double(Vector2::new(e.snap_spacing.x, e.snap_spacing.y));
        self.writer
            .write_2raw_double(Vector2::new(e.grid_spacing.x, e.grid_spacing.y));
        // Circle Zoom BS 72
        self.writer.write_bit_short(e.circle_sides);

        // R2007+: Grid Major BS 61
        if self.version.r2007_plus() {
            self.writer.write_bit_short(if e.grid_major > 0 { e.grid_major } else { 5 });
        }

        // Status/UCS data (written for all versions)
        // Frozen layer count BL
        self.writer.write_bit_long(e.frozen_layers.len() as i32);
        // Status flags BL 90
        self.writer.write_bit_long(e.status.to_bits());
        // Style Sheet TV 1
        self.writer.write_variable_text("");
        // Render Mode RC 281
        self.writer.write_byte(e.render_mode as u8);
        // UCS at origin B 74
        self.writer.write_bit(e.ucs_icon_visible);
        // UCS per viewport B 71
        self.writer.write_bit(e.ucs_per_viewport);
        // UCS Origin 3BD 110
        self.writer.write_3bit_double(e.ucs_origin);
        // UCS X Axis 3BD 111
        self.writer.write_3bit_double(e.ucs_x_axis);
        // UCS Y Axis 3BD 112
        self.writer.write_3bit_double(e.ucs_y_axis);
        // UCS Elevation BD 146
        self.writer.write_bit_double(e.elevation);
        // UCS Ortho View Type BS 79
        self.writer.write_bit_short(e.ucs_ortho_type);

        // R2004+: ShadePlot Mode BS 170
        if self.version.r2004_plus() {
            self.writer.write_bit_short(e.shade_plot_mode);
        }

        // R2007+: lighting + ambient
        if self.version.r2007_plus() {
            self.writer.write_bit(e.default_lighting);
            self.writer.write_byte(e.default_lighting_type as u8);
            self.writer.write_bit_double(e.brightness);
            self.writer.write_bit_double(e.contrast);
            self.writer
                .write_cm_color(&crate::types::Color::from_index(e.ambient_color as i16));
        }

        // Frozen layer handles (written for all versions)
        for h in &e.frozen_layers {
            if self.version.r2004_plus() {
                self.writer
                    .write_handle(DwgReferenceType::SoftPointer, h.value());
            } else {
                self.writer
                    .write_handle(DwgReferenceType::HardPointer, h.value());
            }
        }

        // Clip boundary handle (hard pointer)
        self.writer
            .write_handle(DwgReferenceType::HardPointer, 0);

        // R2000 (AC1015) only: VIEWPORT ENT HEADER
        if self.version == crate::io::dwg::dwg_version::DwgVersion::AC15 {
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
        }

        // Named UCS and Base UCS handles (written for all versions)
        self.writer
            .write_handle(DwgReferenceType::HardPointer, e.ucs_handle.value());
        self.writer
            .write_handle(DwgReferenceType::HardPointer, e.base_ucs_handle.value());

        // R2007+: 4 additional handles
        if self.version.r2007_plus() {
            // Background (soft pointer)
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, 0);
            // Visual Style (hard pointer)
            self.writer
                .write_handle(DwgReferenceType::HardPointer, 0);
            // Shadeplot ID (soft pointer)
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, 0);
            // Sun (hard owner)
            self.writer
                .write_handle(DwgReferenceType::HardOwnership, 0);
        }

        self.register_object(e.common.handle);
    }

    // â”€â”€ Dimension (dispatch) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_dimension(&mut self, dim: &Dimension) {
        match dim {
            Dimension::Linear(d) => self.write_dimension_linear(d),
            Dimension::Aligned(d) => self.write_dimension_aligned(d),
            Dimension::Radius(d) => self.write_dimension_radius(d),
            Dimension::Diameter(d) => self.write_dimension_diameter(d),
            Dimension::Angular2Ln(d) => self.write_dimension_angular_2ln(d),
            Dimension::Angular3Pt(d) => self.write_dimension_angular_3pt(d),
            Dimension::Ordinate(d) => self.write_dimension_ordinate(d),
        }
    }

    /// Write the common dimension data shared by all dimension types.
    fn write_common_dimension_data(
        &mut self,
        type_code: i16,
        base: &DimensionBase,
    ) {
        self.entity_preamble(type_code, &base.common);

        // R2010+: Version RC 280
        if self.version.r2010_plus() {
            self.writer.write_byte(base.version);
        }

        // Extrusion 3BD 210
        self.writer.write_3bit_double(base.normal);
        // Text midpt 2RD 11
        self.writer.write_2raw_double(Vector2::new(
            base.text_middle_point.x,
            base.text_middle_point.y,
        ));
        // Elevation BD 11 Z-coord
        self.writer.write_bit_double(base.text_middle_point.z);

        // Flags byte (0 = text user-defined location)
        self.writer.write_byte(0);

        // User text TV 1
        self.writer.write_variable_text(&base.text);

        // Text rot BD 53
        self.writer.write_bit_double(base.text_rotation);
        // Horiz dir BD 51
        self.writer.write_bit_double(base.horizontal_direction);

        // Insertion scale/rotation (undocumented, all 1.0/0.0)
        self.writer.write_3bit_double(Vector3::new(1.0, 1.0, 1.0));
        self.writer.write_bit_double(0.0);

        // R2000+:
        if self.version.r2000_plus() {
            // Attachment Point BS 71
            self.writer.write_bit_short(base.attachment_point as i16);
            // Linespacing Style BS 72
            self.writer.write_bit_short(1); // 1 = At Least
            // Linespacing Factor BD 41
            self.writer.write_bit_double(base.line_spacing_factor);
            // Actual Measurement BD 42
            self.writer.write_bit_double(base.actual_measurement);
        }

        // R2007+:
        if self.version.r2007_plus() {
            self.writer.write_bit(false); // unknown
            self.writer.write_bit(false); // flip arrow 1
            self.writer.write_bit(false); // flip arrow 2
        }

        // 12-pt 2RD 12
        self.writer
            .write_2raw_double(Vector2::new(base.insertion_point.x, base.insertion_point.y));

        // Dim style handle (hard pointer)
        let ds_handle = self
            .document
            .dim_styles
            .get(&base.style_name)
            .map(|d| d.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, ds_handle.value());

        // Block handle (hard pointer)
        let block_handle = self
            .document
            .block_records
            .get(&base.block_name)
            .map(|br| br.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, block_handle.value());
    }

    fn write_dimension_linear(&mut self, d: &DimensionLinear) {
        self.write_common_dimension_data(common::OBJ_DIMENSION_LINEAR, &d.base);
        self.writer
            .write_3bit_double(d.first_point);
        self.writer
            .write_3bit_double(d.second_point);
        self.writer
            .write_3bit_double(d.definition_point);
        self.writer.write_bit_double(d.ext_line_rotation);
        self.writer.write_bit_double(d.rotation);
        self.register_object(d.base.common.handle);
    }

    fn write_dimension_aligned(&mut self, d: &DimensionAligned) {
        self.write_common_dimension_data(common::OBJ_DIMENSION_ALIGNED, &d.base);
        self.writer
            .write_3bit_double(d.first_point);
        self.writer
            .write_3bit_double(d.second_point);
        self.writer
            .write_3bit_double(d.definition_point);
        self.writer.write_bit_double(d.ext_line_rotation);
        self.register_object(d.base.common.handle);
    }

    fn write_dimension_radius(&mut self, d: &DimensionRadius) {
        self.write_common_dimension_data(common::OBJ_DIMENSION_RADIUS, &d.base);
        self.writer
            .write_3bit_double(d.definition_point);
        self.writer
            .write_3bit_double(d.angle_vertex);
        self.writer.write_bit_double(d.leader_length);
        self.register_object(d.base.common.handle);
    }

    fn write_dimension_diameter(&mut self, d: &DimensionDiameter) {
        self.write_common_dimension_data(common::OBJ_DIMENSION_DIAMETER, &d.base);
        self.writer
            .write_3bit_double(d.definition_point);
        self.writer
            .write_3bit_double(d.angle_vertex);
        self.writer.write_bit_double(d.leader_length);
        self.register_object(d.base.common.handle);
    }

    fn write_dimension_angular_2ln(&mut self, d: &DimensionAngular2Ln) {
        self.write_common_dimension_data(common::OBJ_DIMENSION_ANG_2LN, &d.base);
        self.writer
            .write_2raw_double(Vector2::new(d.dimension_arc.x, d.dimension_arc.y));
        self.writer
            .write_3bit_double(d.first_point);
        self.writer
            .write_3bit_double(d.second_point);
        self.writer
            .write_3bit_double(d.angle_vertex);
        self.writer
            .write_3bit_double(d.definition_point);
        self.register_object(d.base.common.handle);
    }

    fn write_dimension_angular_3pt(&mut self, d: &DimensionAngular3Pt) {
        self.write_common_dimension_data(common::OBJ_DIMENSION_ANG_3PT, &d.base);
        self.writer
            .write_3bit_double(d.definition_point);
        self.writer
            .write_3bit_double(d.first_point);
        self.writer
            .write_3bit_double(d.second_point);
        self.writer
            .write_3bit_double(d.angle_vertex);
        self.register_object(d.base.common.handle);
    }

    fn write_dimension_ordinate(&mut self, d: &DimensionOrdinate) {
        self.write_common_dimension_data(common::OBJ_DIMENSION_ORDINATE, &d.base);
        self.writer
            .write_3bit_double(d.base.definition_point);
        self.writer
            .write_3bit_double(d.feature_location);
        self.writer
            .write_3bit_double(d.leader_endpoint);
        // Ordinate type: 1 = X, 0 = Y
        self.writer.write_byte(if d.is_ordinate_type_x { 1 } else { 0 });
        self.register_object(d.base.common.handle);
    }

    // â”€â”€ Polyline2D â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_polyline2d(&mut self, e: &Polyline2D) {
        self.entity_preamble(common::OBJ_POLYLINE_2D, &e.common);

        self.writer.write_bit_short(e.flags.bits() as i16);
        self.writer.write_bit_short(e.smooth_surface as i16); // BS 75 curve type
        self.writer.write_bit_double(e.start_width);
        self.writer.write_bit_double(e.end_width);
        self.writer.write_bit_thickness(e.thickness);
        self.writer.write_bit_double(e.elevation);
        self.writer.write_bit_extrusion(e.normal);

        // Allocate handles for vertices and seqend
        let vertex_handles: Vec<Handle> = (0..e.vertices.len())
            .map(|_| self.alloc_handle())
            .collect();
        let seqend_handle = self.alloc_handle();

        if self.version.r2004_plus() {
            self.writer.write_bit_long(e.vertices.len() as i32);
        }

        // Vertex handles
        if self.version.r13_15_only() {
            let first = vertex_handles.first().copied().unwrap_or(Handle::NULL);
            // Last owned entity is SEQEND, not the last vertex
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, first.value());
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, seqend_handle.value());
        } else if self.version.r2004_plus() {
            for &vh in &vertex_handles {
                self.writer
                    .write_handle(DwgReferenceType::HardOwnership, vh.value());
            }
        }

        // Seqend handle
        self.writer
            .write_handle(DwgReferenceType::HardOwnership, seqend_handle.value());

        self.register_object(e.common.handle);

        // Write vertices as child entities â€” set up internal entity chain
        let saved_prev = self.prev_handle.take();
        let saved_next = self.next_handle.take();

        let sub_count = vertex_handles.len() + 1; // vertices + seqend
        for (i, (v, &vh)) in e.vertices.iter().zip(vertex_handles.iter()).enumerate() {
            self.prev_handle = if i > 0 { Some(vertex_handles[i - 1]) } else { None };
            self.next_handle = if i + 1 < sub_count {
                if i + 1 < vertex_handles.len() { Some(vertex_handles[i + 1]) } else { Some(seqend_handle) }
            } else { None };
            self.write_vertex2d(v, vh, e.common.handle, &e.common.layer, &e.common.color);
        }

        // Write SEQEND â€” last in polyline chain
        self.prev_handle = vertex_handles.last().copied();
        self.next_handle = None;
        self.write_common_entity_data(
            common::OBJ_SEQEND,
            seqend_handle,
            e.common.handle,
            &e.common.layer,
            &e.common.color,
            &crate::types::LineWeight::ByLayer,
            &crate::types::Transparency::default(),
            false,
            1.0,
            "ByLayer",
            &crate::xdata::ExtendedData::default(),
            &[],
            &None,
        );
        self.register_object(seqend_handle);

        // Restore block-level entity chain
        self.prev_handle = saved_prev;
        self.next_handle = saved_next;
    }

    fn write_vertex2d(
        &mut self,
        v: &Vertex2D,
        vertex_handle: Handle,
        owner: Handle,
        parent_layer: &str,
        parent_color: &crate::types::Color,
    ) {
        self.write_common_entity_data(
            common::OBJ_VERTEX_2D,
            vertex_handle,
            owner,
            parent_layer,
            parent_color,
            &crate::types::LineWeight::ByLayer,
            &crate::types::Transparency::default(),
            false,
            1.0,
            "ByLayer",
            &crate::xdata::ExtendedData::default(),
            &[],
            &None,
        );

        // Flags EC 70 NOT bit-pair-coded
        self.writer.write_byte(v.flags.bits() as u8);

        // Point 3BD 10 â€” Z must be 0.0 (elevation from polyline)
        self.writer.write_bit_double(v.location.x);
        self.writer.write_bit_double(v.location.y);
        self.writer.write_bit_double(0.0);

        // Start width BD 40 â€” negative = compression trick
        if v.start_width != 0.0 && v.end_width == v.start_width {
            self.writer.write_bit_double(-v.start_width);
        } else {
            self.writer.write_bit_double(v.start_width);
            // End width BD 41 â€” only present if start >= 0
            self.writer.write_bit_double(v.end_width);
        }

        // Bulge BD 42
        self.writer.write_bit_double(v.bulge);

        // R2010+: Vertex ID BL 91
        if self.version.r2010_plus() {
            self.writer.write_bit_long(v.id);
        }

        // Tangent dir BD 50
        self.writer.write_bit_double(v.curve_tangent);

        self.register_object(vertex_handle);
    }

    // â”€â”€ Polyline3D â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_polyline3d(&mut self, e: &Polyline3D) {
        self.entity_preamble(common::OBJ_POLYLINE_3D, &e.common);

        // Byte 1: smooth surface type (C# hardcodes 0)
        self.writer.write_byte(e.smooth_type as u8);
        // Byte 2: closed flag only â€” bit 3 (Is3DPolyline) is implied by
        // the object type code and must NOT be written in the DWG data
        let closed_flag = if e.flags.closed { 1u8 } else { 0u8 };
        self.writer.write_byte(closed_flag);

        // Allocate handles for any vertex that doesn't have one
        let vertex_handles: Vec<Handle> = e.vertices.iter().map(|v| {
            if v.handle.is_null() { self.alloc_handle() } else { v.handle }
        }).collect();
        let seqend_handle = self.alloc_handle();

        if self.version.r2004_plus() {
            self.writer.write_bit_long(e.vertices.len() as i32);
        }

        // Vertex handles
        if self.version.r13_15_only() {
            let first = vertex_handles.first().copied().unwrap_or(Handle::NULL);
            // Last owned entity is SEQEND, not the last vertex
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, first.value());
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, seqend_handle.value());
        } else if self.version.r2004_plus() {
            for &vh in &vertex_handles {
                self.writer
                    .write_handle(DwgReferenceType::HardOwnership, vh.value());
            }
        }

        // Seqend
        self.writer
            .write_handle(DwgReferenceType::HardOwnership, seqend_handle.value());

        self.register_object(e.common.handle);

        // Write vertices â€” set up internal entity chain
        let saved_prev = self.prev_handle.take();
        let saved_next = self.next_handle.take();

        let sub_count = vertex_handles.len() + 1; // vertices + seqend
        for (i, (v, &vh)) in e.vertices.iter().zip(vertex_handles.iter()).enumerate() {
            self.prev_handle = if i > 0 { Some(vertex_handles[i - 1]) } else { None };
            self.next_handle = if i + 1 < sub_count {
                if i + 1 < vertex_handles.len() { Some(vertex_handles[i + 1]) } else { Some(seqend_handle) }
            } else { None };
            self.write_vertex3d(v, vh, e.common.handle, &e.common.layer, &e.common.color);
        }

        // Write SEQEND â€” last in polyline chain
        self.prev_handle = vertex_handles.last().copied();
        self.next_handle = None;
        self.write_common_entity_data(
            common::OBJ_SEQEND,
            seqend_handle,
            e.common.handle,
            &e.common.layer,
            &e.common.color,
            &crate::types::LineWeight::ByLayer,
            &crate::types::Transparency::default(),
            false,
            1.0,
            "ByLayer",
            &crate::xdata::ExtendedData::default(),
            &[],
            &None,
        );
        self.register_object(seqend_handle);

        // Restore block-level entity chain
        self.prev_handle = saved_prev;
        self.next_handle = saved_next;
    }

    fn write_vertex3d(
        &mut self,
        v: &Vertex3DPolyline,
        vertex_handle: Handle,
        owner: Handle,
        parent_layer: &str,
        parent_color: &crate::types::Color,
    ) {
        self.write_common_entity_data(
            common::OBJ_VERTEX_3D,
            vertex_handle,
            owner,
            parent_layer,
            parent_color,
            &crate::types::LineWeight::ByLayer,
            &crate::types::Transparency::default(),
            false,
            1.0,
            "ByLayer",
            &crate::xdata::ExtendedData::default(),
            &[],
            &None,
        );

        self.writer
            .write_byte(v.flags as u8); // Flags EC 70
        self.writer.write_3bit_double(v.position);
        self.register_object(vertex_handle);
    }

    // â”€â”€ PolyfaceMesh â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_polyface_mesh(&mut self, e: &PolyfaceMesh) {
        self.entity_preamble(common::OBJ_POLYLINE_PFACE, &e.common);

        self.writer
            .write_bit_short(e.vertices.len() as i16);
        self.writer
            .write_bit_short(e.faces.len() as i16);

        // Allocate handles for vertices and faces that don't have one
        let vertex_handles: Vec<Handle> = e.vertices.iter().map(|v| {
            if v.common.handle.is_null() { self.alloc_handle() } else { v.common.handle }
        }).collect();
        let face_handles: Vec<Handle> = e.faces.iter().map(|f| {
            if f.common.handle.is_null() { self.alloc_handle() } else { f.common.handle }
        }).collect();
        let seqend_handle = self.alloc_handle();

        let total_owned = e.vertices.len() + e.faces.len();

        if self.version.r2004_plus() {
            self.writer.write_bit_long(total_owned as i32);
        }

        if self.version.r13_15_only() {
            // First / last child â€” last is always SEQEND
            let first = vertex_handles.first()
                .or_else(|| face_handles.first())
                .copied()
                .unwrap_or(Handle::NULL);
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, first.value());
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, seqend_handle.value());
        } else if self.version.r2004_plus() {
            for &vh in &vertex_handles {
                self.writer
                    .write_handle(DwgReferenceType::HardOwnership, vh.value());
            }
            for &fh in &face_handles {
                self.writer
                    .write_handle(DwgReferenceType::HardOwnership, fh.value());
            }
        }

        // Seqend
        self.writer
            .write_handle(DwgReferenceType::HardOwnership, seqend_handle.value());

        self.register_object(e.common.handle);

        // Build combined sub-entity handle chain for prev/next linking
        let saved_prev = self.prev_handle.take();
        let saved_next = self.next_handle.take();

        let mut all_sub_handles: Vec<Handle> = Vec::with_capacity(total_owned + 1);
        all_sub_handles.extend_from_slice(&vertex_handles);
        all_sub_handles.extend_from_slice(&face_handles);
        all_sub_handles.push(seqend_handle);

        let mut sub_idx = 0usize;

        // Write vertex child entities (OBJ_VERTEX_PFACE = 13)
        for (v, &vh) in e.vertices.iter().zip(vertex_handles.iter()) {
            self.prev_handle = if sub_idx > 0 { Some(all_sub_handles[sub_idx - 1]) } else { None };
            self.next_handle = if sub_idx + 1 < all_sub_handles.len() { Some(all_sub_handles[sub_idx + 1]) } else { None };
            self.write_common_entity_data(
                common::OBJ_VERTEX_PFACE,
                vh,
                e.common.handle,
                &e.common.layer,
                &e.common.color,
                &crate::types::LineWeight::ByLayer,
                &crate::types::Transparency::default(),
                false,
                1.0,
                "ByLayer",
                &crate::xdata::ExtendedData::default(),
                &[],
                &None,
            );
            self.writer.write_byte(v.flags.bits() as u8);
            self.writer.write_3bit_double(v.location);
            self.register_object(vh);
            sub_idx += 1;
        }

        // Write face child entities (OBJ_VERTEX_PFACE_FACE = 14)
        for (f, &fh) in e.faces.iter().zip(face_handles.iter()) {
            self.prev_handle = if sub_idx > 0 { Some(all_sub_handles[sub_idx - 1]) } else { None };
            self.next_handle = if sub_idx + 1 < all_sub_handles.len() { Some(all_sub_handles[sub_idx + 1]) } else { None };
            self.write_common_entity_data(
                common::OBJ_VERTEX_PFACE_FACE,
                fh,
                e.common.handle,
                &e.common.layer,
                &e.common.color,
                &crate::types::LineWeight::ByLayer,
                &crate::types::Transparency::default(),
                false,
                1.0,
                "ByLayer",
                &crate::xdata::ExtendedData::default(),
                &[],
                &None,
            );
            self.writer.write_bit_short(f.index1);
            self.writer.write_bit_short(f.index2);
            self.writer.write_bit_short(f.index3);
            self.writer.write_bit_short(f.index4);
            self.register_object(fh);
            sub_idx += 1;
        }

        // Write SEQEND â€” last in polyface chain
        self.prev_handle = if sub_idx > 0 { Some(all_sub_handles[sub_idx - 1]) } else { None };
        self.next_handle = None;
        self.write_common_entity_data(
            common::OBJ_SEQEND,
            seqend_handle,
            e.common.handle,
            &e.common.layer,
            &e.common.color,
            &crate::types::LineWeight::ByLayer,
            &crate::types::Transparency::default(),
            false,
            1.0,
            "ByLayer",
            &crate::xdata::ExtendedData::default(),
            &[],
            &None,
        );
        self.register_object(seqend_handle);

        // Restore block-level entity chain
        self.prev_handle = saved_prev;
        self.next_handle = saved_next;
    }

    // â”€â”€ PolygonMesh â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_polygon_mesh(&mut self, e: &PolygonMeshEntity) {
        self.entity_preamble(common::OBJ_POLYLINE_MESH, &e.common);

        self.writer.write_bit_short(e.flags.bits() as i16);
        self.writer.write_bit_short(e.smooth_type as i16);
        self.writer.write_bit_short(e.m_vertex_count);
        self.writer.write_bit_short(e.n_vertex_count);
        self.writer.write_bit_short(e.m_smooth_density);
        self.writer.write_bit_short(e.n_smooth_density);

        // Allocate handles for vertices that don't have one
        let vertex_handles: Vec<Handle> = e.vertices.iter().map(|v| {
            if v.common.handle.is_null() { self.alloc_handle() } else { v.common.handle }
        }).collect();
        let seqend_handle = self.alloc_handle();

        if self.version.r2004_plus() {
            self.writer
                .write_bit_long(e.vertices.len() as i32);
        }

        if self.version.r13_15_only() {
            let first = vertex_handles.first().copied().unwrap_or(Handle::NULL);
            // Last owned entity is SEQEND, not the last vertex
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, first.value());
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, seqend_handle.value());
        } else if self.version.r2004_plus() {
            for &vh in &vertex_handles {
                self.writer
                    .write_handle(DwgReferenceType::HardOwnership, vh.value());
            }
        }

        // Seqend
        self.writer
            .write_handle(DwgReferenceType::HardOwnership, seqend_handle.value());

        self.register_object(e.common.handle);

        // Write vertex child entities (OBJ_VERTEX_MESH = 12) with internal chain
        let saved_prev = self.prev_handle.take();
        let saved_next = self.next_handle.take();

        let sub_count = vertex_handles.len() + 1; // vertices + seqend
        for (i, (v, &vh)) in e.vertices.iter().zip(vertex_handles.iter()).enumerate() {
            self.prev_handle = if i > 0 { Some(vertex_handles[i - 1]) } else { None };
            self.next_handle = if i + 1 < sub_count {
                if i + 1 < vertex_handles.len() { Some(vertex_handles[i + 1]) } else { Some(seqend_handle) }
            } else { None };
            self.write_common_entity_data(
                common::OBJ_VERTEX_MESH,
                vh,
                e.common.handle,
                &e.common.layer,
                &e.common.color,
                &crate::types::LineWeight::ByLayer,
                &crate::types::Transparency::default(),
                false,
                1.0,
                "ByLayer",
                &crate::xdata::ExtendedData::default(),
                &[],
                &None,
            );
            self.writer.write_byte(v.flags as u8);
            self.writer.write_3bit_double(v.location);
            self.register_object(vh);
        }

        // Write SEQEND â€” last in polygon mesh chain
        self.prev_handle = vertex_handles.last().copied();
        self.next_handle = None;
        self.write_common_entity_data(
            common::OBJ_SEQEND,
            seqend_handle,
            e.common.handle,
            &e.common.layer,
            &e.common.color,
            &crate::types::LineWeight::ByLayer,
            &crate::types::Transparency::default(),
            false,
            1.0,
            "ByLayer",
            &crate::xdata::ExtendedData::default(),
            &[],
            &None,
        );
        self.register_object(seqend_handle);

        // Restore block-level entity chain
        self.prev_handle = saved_prev;
        self.next_handle = saved_next;
    }

    // â”€â”€ Seqend â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_seqend(&mut self, e: &Seqend) {
        self.entity_preamble(common::OBJ_SEQEND, &e.common);
        self.register_object(e.common.handle);
    }

    // â”€â”€ Mesh (ACAD_MESH) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_mesh(&mut self, e: &Mesh) {
        // UNLISTED entity type â€” always use DXF class number (500+)
        let type_code = self.class_type_code("MESH", common::OBJ_MESH);
        self.entity_preamble(type_code, &e.common);

        // 71 BS Version
        self.writer.write_bit_short(e.version);
        // 72 B BlendCrease (BIT, not byte!)
        self.writer.write_bit(e.blend_crease);
        // 91 BL SubdivisionLevel
        self.writer.write_bit_long(e.subdivision_level);

        // 92 BL nvertices
        self.writer.write_bit_long(e.vertices.len() as i32);
        for v in &e.vertices {
            // 10 3BD vertice
            self.writer.write_3bit_double(*v);
        }

        // Faces: count = sum of (1 + face.vertices.len()) for each face
        let nfaces: i32 = e.faces.iter().map(|f| 1 + f.vertices.len() as i32).sum();
        self.writer.write_bit_long(nfaces);
        for face in &e.faces {
            self.writer.write_bit_long(face.vertices.len() as i32);
            for idx in &face.vertices {
                self.writer.write_bit_long(*idx as i32);
            }
        }

        // Edges
        self.writer.write_bit_long(e.edges.len() as i32);
        for edge in &e.edges {
            self.writer.write_bit_long(edge.start as i32);
            self.writer.write_bit_long(edge.end as i32);
        }

        // Crease values: must write for EVERY edge, use 0 if no crease
        self.writer.write_bit_long(e.edges.len() as i32);
        for edge in &e.edges {
            let crease = edge.crease.unwrap_or(0.0);
            self.writer.write_bit_double(crease);
        }

        // Trailing value (override option for meshes)
        self.writer.write_bit_long(0);

        self.register_object(e.common.handle);
    }

    // â”€â”€ MLine â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_mline(&mut self, e: &MLine) {
        self.entity_preamble(common::OBJ_MLINE, &e.common);

        self.writer.write_bit_double(e.scale_factor);
        self.writer.write_byte(e.justification as u8);
        self.writer.write_3bit_double(e.start_point);
        self.writer.write_3bit_double(e.normal);
        
        // Openclosed BS: open (1), closed (3) â€” always has HAS_VERTICES flag
        let flag_value: i16 = if e.flags.contains(MLineFlags::CLOSED) { 3 } else { 1 };
        self.writer.write_bit_short(flag_value);

        // Linesinstyle RC 73 â€” number of segments from first vertex
        let nlines: u8 = if let Some(first_v) = e.vertices.first() {
            first_v.segments.len() as u8
        } else {
            e.style_element_count as u8
        };
        self.writer.write_byte(nlines);

        // Vertices
        self.writer
            .write_bit_short(e.vertices.len() as i16);
        for v in &e.vertices {
            self.writer.write_3bit_double(v.position);
            self.writer.write_3bit_double(v.direction);
            self.writer.write_3bit_double(v.miter);

            for seg in &v.segments {
                self.writer
                    .write_bit_short(seg.parameters.len() as i16);
                for p in &seg.parameters {
                    self.writer.write_bit_double(*p);
                }
                self.writer
                    .write_bit_short(seg.area_fill_parameters.len() as i16);
                for p in &seg.area_fill_parameters {
                    self.writer.write_bit_double(*p);
                }
            }
        }

        // MLine style handle â€” fall back to document's current MLine style
        let sh = e.style_handle
            .filter(|h| !h.is_null())
            .unwrap_or(self.document.header.current_multiline_style_handle);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, sh.value());

        self.register_object(e.common.handle);
    }

    // â”€â”€ RasterImage â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_raster_image(&mut self, e: &RasterImage) {
        // UNLISTED entity type â€” always use DXF class number (500+)
        let type_code = self.class_type_code("IMAGE", common::OBJ_IMAGE);
        self.entity_preamble(type_code, &e.common);

        self.writer.write_bit_long(e.class_version);
        self.writer.write_3bit_double(e.insertion_point);
        self.writer.write_3bit_double(e.u_vector);
        self.writer.write_3bit_double(e.v_vector);
        self.writer
            .write_2raw_double(e.size);
        self.writer.write_bit_short(e.flags.bits() as i16);
        self.writer.write_bit(e.clipping_enabled);
        self.writer.write_byte(e.brightness);
        self.writer.write_byte(e.contrast);
        self.writer.write_byte(e.fade);

        if self.version.r2010_plus() {
            self.writer.write_bit(false); // clip is inverted
        }

        // Clip boundary
        self.write_clip_boundary(&e.clip_boundary);

        // Image def handle
        let def = e.definition_handle.unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, def.value());

        // Image def reactor handle
        let reactor = e.definition_reactor_handle.unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, reactor.value());

        self.register_object(e.common.handle);
    }

    fn write_clip_boundary(&mut self, clip: &ClipBoundary) {
        self.writer.write_bit_short(clip.clip_type as i16);
        
        match clip.clip_type {
            ClipType::Rectangular => {
                // Rectangular clips: exactly 2 vertices, no count written
                if clip.vertices.len() >= 2 {
                    self.writer.write_2raw_double(clip.vertices[0]);
                    self.writer.write_2raw_double(clip.vertices[1]);
                } else {
                    // Default to origin
                    self.writer.write_2raw_double(Vector2::ZERO);
                    self.writer.write_2raw_double(Vector2::ZERO);
                }
            }
            ClipType::Polygonal => {
                // Polygonal clips: count + vertices
                self.writer.write_bit_long(clip.vertices.len() as i32);
                for v in &clip.vertices {
                    self.writer.write_2raw_double(*v);
                }
            }
        }
    }

    // â”€â”€ Wipeout â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_wipeout(&mut self, e: &Wipeout) {
        // UNLISTED entity type â€” always use DXF class number (500+)
        // Wipeout uses the "WIPEOUT" DXF class name
        let type_code = self.class_type_code("WIPEOUT", common::OBJ_IMAGE);
        self.entity_preamble(type_code, &e.common);

        self.writer.write_bit_long(e.class_version);
        self.writer.write_3bit_double(e.insertion_point);
        self.writer.write_3bit_double(e.u_vector);
        self.writer.write_3bit_double(e.v_vector);
        self.writer.write_2raw_double(e.size);
        self.writer.write_bit_short(e.flags.bits() as i16);
        self.writer.write_bit(e.clipping_enabled);
        self.writer.write_byte(e.brightness);
        self.writer.write_byte(e.contrast);
        self.writer.write_byte(e.fade);

        if self.version.r2010_plus() {
            self.writer.write_bit(false);
        }

        // Clip boundary
        self.writer
            .write_bit_short(e.clip_type as i16);
        self.writer
            .write_bit_long(e.clip_boundary_vertices.len() as i32);
        for v in &e.clip_boundary_vertices {
            self.writer.write_2raw_double(*v);
        }

        // Definition + reactor handles
        let def = e.definition_handle.unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, def.value());
        let reactor = e.definition_reactor_handle.unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, reactor.value());

        self.register_object(e.common.handle);
    }

    // â”€â”€ OLE2Frame â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_ole2frame(&mut self, e: &Ole2Frame) {
        self.entity_preamble(common::OBJ_OLE2FRAME, &e.common);

        // Flags BS 70
        self.writer.write_bit_short(e.version as i16);

        // R2000+: Mode BS
        if self.version.r2000_plus() {
            self.writer.write_bit_short(0);
        }

        // Data Length BL + data bytes
        self.writer
            .write_bit_long(e.binary_data.len() as i32);
        self.writer
            .write_bytes(&e.binary_data);

        // R2000+: trailing byte
        if self.version.r2000_plus() {
            self.writer.write_byte(3);
        }

        self.register_object(e.common.handle);
    }

    // â”€â”€ MultiLeader â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_multileader(&mut self, e: &MultiLeader) {
        // UNLISTED entity type â€” always use DXF class number (500+)
        let type_code = self.class_type_code("MULTILEADER", common::OBJ_MULTILEADER);
        self.entity_preamble(type_code, &e.common);

        // R2010+: version 2
        if self.version.r2010_plus() {
            self.writer.write_bit_short(2);
        }

        // Write annotation context sub-object FIRST
        self.write_multileader_annotation_context(&e.context, true);

        // === MultiLeader Common Data ===
        
        // 340 Leader StyleId (handle) - HardPointer
        let style = e.style_handle.unwrap_or(Handle::NULL);
        self.writer.write_handle(DwgReferenceType::HardPointer, style.value());
        
        // 90 Property Override Flags (BL)
        self.writer.write_bit_long(e.property_override_flags.bits() as i32);
        
        // 170 LeaderLineType / PathType (BS)
        self.writer.write_bit_short(e.path_type as i16);
        
        // 91 Leader LineColor (CMC)
        self.writer.write_cm_color(&e.line_color);
        
        // 341 LeaderLineTypeID (handle) - HardPointer
        let lt = e.line_type_handle.unwrap_or(Handle::NULL);
        self.writer.write_handle(DwgReferenceType::HardPointer, lt.value());
        
        // 171 LeaderLine Weight (BL not BS!)
        self.writer.write_bit_long(e.line_weight.as_i16() as i32);
        
        // 290 Enable Landing (B)
        self.writer.write_bit(e.enable_landing);
        
        // 291 Enable Dogleg (B)
        self.writer.write_bit(e.enable_dogleg);
        
        // 41 Dogleg Length / Landing distance (BD)
        self.writer.write_bit_double(e.dogleg_length);
        
        // 342 Arrowhead ID (handle) - HardPointer
        let ah = e.arrowhead_handle.unwrap_or(Handle::NULL);
        self.writer.write_handle(DwgReferenceType::HardPointer, ah.value());
        
        // 42 Arrowhead Size (BD)
        self.writer.write_bit_double(e.arrowhead_size);
        
        // 172 Content Type (BS)
        self.writer.write_bit_short(e.content_type as i16);
        
        // 343 Text Style ID (handle) - HardPointer
        let ts = e.text_style_handle.unwrap_or(Handle::NULL);
        self.writer.write_handle(DwgReferenceType::HardPointer, ts.value());
        
        // 173 Text Left Attachment Type (BS)
        self.writer.write_bit_short(e.text_left_attachment as i16);
        
        // 95 Text Right Attachment Type (BS)
        self.writer.write_bit_short(e.text_right_attachment as i16);
        
        // 174 Text Angle Type (BS)
        self.writer.write_bit_short(e.text_angle_type as i16);
        
        // 175 Text Alignment Type (BS)
        self.writer.write_bit_short(e.text_alignment as i16);
        
        // 92 Text Color (CMC)
        self.writer.write_cm_color(&e.text_color);
        
        // 292 Enable Frame Text (B)
        self.writer.write_bit(e.text_frame);
        
        // 344 Block Content ID (handle) - HardPointer
        let bc = e.block_content_handle.unwrap_or(Handle::NULL);
        self.writer.write_handle(DwgReferenceType::HardPointer, bc.value());
        
        // 93 Block Content Color (CMC)
        self.writer.write_cm_color(&e.block_content_color);
        
        // 10 Block Content Scale (3BD)
        self.writer.write_3bit_double(e.block_scale);
        
        // 43 Block Content Rotation (BD)
        self.writer.write_bit_double(e.block_rotation);
        
        // 176 Block Content Connection Type (BS)
        self.writer.write_bit_short(e.block_connection_type as i16);
        
        // 293 Enable Annotation Scale / Is annotative (B)
        self.writer.write_bit(e.enable_annotation_scale);

        // Block Attributes
        self.writer.write_bit_long(e.block_attributes.len() as i32);
        for ba in &e.block_attributes {
            // 330 Block Attribute definition handle (hard pointer)
            let def = ba.attribute_definition_handle.unwrap_or(Handle::NULL);
            self.writer.write_handle(DwgReferenceType::HardPointer, def.value());
            // 302 Block Attribute Text String
            self.writer.write_variable_text(&ba.text);
            // 177 Block Attribute Index
            self.writer.write_bit_short(ba.index);
            // 44 Block Attribute Width
            self.writer.write_bit_double(ba.width);
        }

        // 294 Text Direction Negative (B)
        self.writer.write_bit(e.text_direction_negative);
        
        // 178 Text Align in IPE (BS)
        self.writer.write_bit_short(e.text_align_in_ipe);
        
        // 179 Text Attachment Point (BS)
        self.writer.write_bit_short(e.text_attachment_point as i16);
        
        // 45 ScaleFactor (BD)
        self.writer.write_bit_double(e.scale_factor);

        // R2010+ fields
        if self.version.r2010_plus() {
            // 271 Text attachment direction (BS)
            self.writer.write_bit_short(e.text_attachment_direction as i16);
            // 272 Bottom text attachment direction (BS)
            self.writer.write_bit_short(e.text_bottom_attachment as i16);
            // 273 Top text attachment direction (BS)
            self.writer.write_bit_short(e.text_top_attachment as i16);
        }

        // R2013+ field
        if self.version.r2013_plus(self.dxf_version) {
            // 295 Leader extended to text (B)
            self.writer.write_bit(e.extend_leader_to_text);
        }

        self.register_object(e.common.handle);
    }

    fn write_multileader_annotation_context(&mut self, ctx: &MultiLeaderAnnotContext, write_leader_roots_count: bool) {
        let leader_root_count = ctx.leader_roots.len();
        
        if write_leader_roots_count {
            // BL - Number of leader roots
            self.writer.write_bit_long(leader_root_count as i32);
        } else {
            self.writer.write_bit_long(0);
            self.writer.write_bit(false); // b0
            self.writer.write_bit(false); // b1
            self.writer.write_bit(false); // b2
            self.writer.write_bit(false); // b3
            self.writer.write_bit(false); // b4
            self.writer.write_bit(leader_root_count == 2); // b5
            self.writer.write_bit(leader_root_count == 1); // b6
        }

        // Write each leader root
        for root in &ctx.leader_roots {
            self.write_leader_root(root);
        }

        // === Common data ===
        
        // BD 40 Overall scale
        self.writer.write_bit_double(ctx.scale_factor);
        // 3BD 10 Content base point
        self.writer.write_3bit_double(ctx.content_base_point);
        // BD 41 Text height
        self.writer.write_bit_double(ctx.text_height);
        // BD 140 Arrow head size
        self.writer.write_bit_double(ctx.arrowhead_size);
        // BD 145 Landing gap
        self.writer.write_bit_double(ctx.landing_gap);
        // BS 174 Style left text attachment type
        self.writer.write_bit_short(ctx.text_left_attachment as i16);
        // BS 175 Style right text attachment type
        self.writer.write_bit_short(ctx.text_right_attachment as i16);
        // BS 176 Text align type
        self.writer.write_bit_short(ctx.text_alignment as i16);
        // BS 177 Attachment type (content extents or insertion point)
        self.writer.write_bit_short(ctx.block_connection_type as i16);

        // B 290 Has text contents
        self.writer.write_bit(ctx.has_text_contents);
        
        if ctx.has_text_contents {
            // TV 304 Text label
            self.writer.write_variable_text(&ctx.text_string);
            // 3BD 11 Normal vector
            self.writer.write_3bit_double(ctx.text_normal);
            // H 340 Text style handle (hard pointer)
            let ts = ctx.text_style_handle.unwrap_or(Handle::NULL);
            self.writer.write_handle(DwgReferenceType::HardPointer, ts.value());
            // 3BD 12 Location
            self.writer.write_3bit_double(ctx.text_location);
            // 3BD 13 Direction
            self.writer.write_3bit_double(ctx.text_direction);
            // BD 42 Rotation (radians)
            self.writer.write_bit_double(ctx.text_rotation);
            // BD 43 Boundary width
            self.writer.write_bit_double(ctx.text_width);
            // BD 44 Boundary height
            self.writer.write_bit_double(ctx.text_boundary_height);
            // BD 45 Line spacing factor
            self.writer.write_bit_double(ctx.line_spacing_factor);
            // BS 170 Line spacing style
            self.writer.write_bit_short(ctx.line_spacing_style as i16);
            // CMC 90 Text color
            self.writer.write_cm_color(&ctx.text_color);
            // BS 171 Alignment / Text Attachment Point
            self.writer.write_bit_short(ctx.text_attachment_point as i16);
            // BS 172 Flow direction
            self.writer.write_bit_short(ctx.text_flow_direction as i16);
            // CMC 91 Background fill color
            self.writer.write_cm_color(&ctx.background_fill_color);
            // BD 141 Background scale factor
            self.writer.write_bit_double(ctx.background_scale_factor);
            // BL 92 Background transparency
            self.writer.write_bit_long(ctx.background_transparency);
            // B 291 Is background fill enabled
            self.writer.write_bit(ctx.background_fill_enabled);
            // B 292 Is background mask fill on
            self.writer.write_bit(ctx.background_mask_fill_on);
            // BS 173 Column type
            self.writer.write_bit_short(ctx.column_type);
            // B 293 Is text height automatic
            self.writer.write_bit(ctx.text_height_automatic);
            // BD 142 Column width
            self.writer.write_bit_double(ctx.column_width);
            // BD 143 Column gutter
            self.writer.write_bit_double(ctx.column_gutter);
            // B 294 Column flow reversed
            self.writer.write_bit(ctx.column_flow_reversed);
            
            // Column sizes (BL count + BD values)
            self.writer.write_bit_long(ctx.column_sizes.len() as i32);
            for size in &ctx.column_sizes {
                self.writer.write_bit_double(*size);
            }
            
            // B 295 Word break
            self.writer.write_bit(ctx.word_break);
            // B Unknown
            self.writer.write_bit(false);
        } else {
            // B 296 Has contents block - only written when has_text_contents is false
            self.writer.write_bit(ctx.has_block_contents);

            if ctx.has_block_contents {
                // H 341 Block table record handle (soft pointer)
                let bh = ctx.block_content_handle.unwrap_or(Handle::NULL);
                self.writer.write_handle(DwgReferenceType::SoftPointer, bh.value());
                // 3BD 14 Normal vector
                self.writer.write_3bit_double(ctx.block_content_normal);
                // 3BD 15 Location
                self.writer.write_3bit_double(ctx.block_content_location);
                // 3BD 16 Scale vector
                self.writer.write_3bit_double(ctx.block_content_scale);
                // BD 46 Rotation (radians)
                self.writer.write_bit_double(ctx.block_rotation);
                // CMC 93 Block color
                self.writer.write_cm_color(&ctx.block_content_color);
                
                // BD (16) 47 - 16 doubles for transformation matrix
                for i in 0..16 {
                    self.writer.write_bit_double(ctx.transform_matrix[i]);
                }
            }
        }

        // 3BD 110 Base point
        self.writer.write_3bit_double(ctx.base_point);
        // 3BD 111 Base direction
        self.writer.write_3bit_double(ctx.base_direction);
        // 3BD 112 Base vertical
        self.writer.write_3bit_double(ctx.base_vertical);
        // B 297 Is normal reversed
        self.writer.write_bit(ctx.normal_reversed);

        // R2010+ fields
        if self.version.r2010_plus() {
            // BS 273 Style top attachment
            self.writer.write_bit_short(ctx.text_top_attachment as i16);
            // BS 272 Style bottom attachment
            self.writer.write_bit_short(ctx.text_bottom_attachment as i16);
        }
    }

    fn write_leader_root(&mut self, root: &LeaderRoot) {
        // B 290 Is content valid
        self.writer.write_bit(root.content_valid);
        // B 291 Unknown (ODA writes true)
        self.writer.write_bit(root.unknown);
        // 3BD 10 Connection point
        self.writer.write_3bit_double(root.connection_point);
        // 3BD 11 Direction
        self.writer.write_3bit_double(root.direction);

        // Break start/end point pairs
        self.writer.write_bit_long(root.break_points.len() as i32);
        for pair in &root.break_points {
            // 3BD 12 Break start point
            self.writer.write_3bit_double(pair.start_point);
            // 3BD 13 Break end point
            self.writer.write_3bit_double(pair.end_point);
        }

        // BL 90 Leader index
        self.writer.write_bit_long(root.leader_index);
        // BD 40 Landing distance
        self.writer.write_bit_double(root.landing_distance);

        // Leader lines
        self.writer.write_bit_long(root.lines.len() as i32);
        for line in &root.lines {
            self.write_leader_line(line);
        }

        // R2010+
        if self.version.r2010_plus() {
            // BS 271 Attachment direction
            self.writer.write_bit_short(root.text_attachment_direction as i16);
        }
    }

    fn write_leader_line(&mut self, line: &LeaderLine) {
        // Points
        self.writer.write_bit_long(line.points.len() as i32);
        for pt in &line.points {
            self.writer.write_3bit_double(*pt);
        }

        // Break info
        self.writer.write_bit_long(line.break_info_count);
        if line.break_info_count > 0 {
            // BL 90 Segment index
            self.writer.write_bit_long(line.segment_index);
            // Start/end point pairs
            self.writer.write_bit_long(line.break_points.len() as i32);
            for sep in &line.break_points {
                self.writer.write_3bit_double(sep.start_point);
                self.writer.write_3bit_double(sep.end_point);
            }
        }

        // BL 91 Leader line index
        self.writer.write_bit_long(line.index);

        // R2010+ line properties
        if self.version.r2010_plus() {
            // BS 170 Leader type (path type)
            self.writer.write_bit_short(line.path_type as i16);
            // CMC 92 Line color
            self.writer.write_cm_color(&line.line_color);
            // H 340 Line type handle (hard pointer)
            let lt = line.line_type_handle.unwrap_or(Handle::NULL);
            self.writer.write_handle(DwgReferenceType::HardPointer, lt.value());
            // BL 171 Line weight
            self.writer.write_bit_long(line.line_weight.as_i16() as i32);
            // BD 40 Arrow size
            self.writer.write_bit_double(line.arrowhead_size);
            // H 341 Arrow symbol handle (hard pointer)
            let ah = line.arrowhead_handle.unwrap_or(Handle::NULL);
            self.writer.write_handle(DwgReferenceType::HardPointer, ah.value());
            // BL 93 Override flags
            self.writer.write_bit_long(line.override_flags.bits() as i32);
        }
    }

    // â”€â”€ Attribute Definition â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_attribute_definition(&mut self, e: &AttributeDefinition) {
        self.entity_preamble(common::OBJ_ATTDEF, &e.common);

        // writeTextEntity portion
        self.write_text_entity_data(
            e.insertion_point,
            e.alignment_point,
            e.normal,
            0.0, // thickness
            e.oblique_angle,
            e.rotation,
            e.height,
            e.width_factor,
            &e.default_value,
            0,  // generation (text mirror flags)
            e.horizontal_alignment as i16,
            e.vertical_alignment as i16,
        );

        // Style handle (comes from writeTextEntity)
        let style_handle = self
            .document
            .text_styles
            .get(&e.text_style)
            .map(|s| s.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, style_handle.value());

        // writeCommonAttData: R2010+ version byte
        if self.version.r2010_plus() {
            self.writer.write_byte(0); // version
        }

        // R2018+: AttributeType byte
        if self.version.r2018_plus(self.dxf_version) {
            // AttributeType: 1 = SingleLine, 2 = MultiLine, 4 = ConstantMultiLine
            self.writer.write_byte(1); // SingleLine = 1 (per C# enum), no MText content follows
        }

        // Tag, field length, flags
        self.writer.write_variable_text(&e.tag);
        self.writer.write_bit_short(e.field_length);
        let flag_byte = e.flags.to_bits();
        self.writer.write_byte(flag_byte as u8);

        // R2007+: lock position
        if self.version.r2007_plus() {
            self.writer.write_bit(false);
        }

        // writeAttDefinition: R2010+ version byte (second)
        if self.version.r2010_plus() {
            self.writer.write_byte(0);
        }

        // Prompt
        self.writer.write_variable_text(&e.prompt);

        self.register_object(e.common.handle);
    }

    // â”€â”€ Attribute Entity â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_attribute_entity(&mut self, e: &AttributeEntity) {
        self.entity_preamble(common::OBJ_ATTRIB, &e.common);

        // writeTextEntity portion
        self.write_text_entity_data(
            e.insertion_point,
            e.alignment_point,
            e.normal,
            0.0, // thickness
            e.oblique_angle,
            e.rotation,
            e.height,
            e.width_factor,
            &e.value,
            0,  // generation (text mirror flags)
            e.horizontal_alignment as i16,
            e.vertical_alignment as i16,
        );

        // Style handle (comes from writeTextEntity)
        let style_handle = self
            .document
            .text_styles
            .get(&e.text_style)
            .map(|s| s.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::HardPointer, style_handle.value());

        // writeCommonAttData: R2010+ version byte
        if self.version.r2010_plus() {
            self.writer.write_byte(0); // version
        }

        // R2018+: AttributeType byte
        if self.version.r2018_plus(self.dxf_version) {
            // AttributeType: 1 = SingleLine, 2 = MultiLine, 4 = ConstantMultiLine
            self.writer.write_byte(1); // SingleLine = 1 (per C# enum), no MText content follows
        }

        // Tag, field length, flags
        self.writer.write_variable_text(&e.tag);
        self.writer.write_bit_short(e.field_length);
        let flag_byte = e.flags.to_bits();
        self.writer.write_byte(flag_byte as u8);

        // R2007+: lock position
        if self.version.r2007_plus() {
            self.writer.write_bit(false);
        }

        // Style handle (already written above in writeTextEntity)
        self.register_object(e.common.handle);
    }

    // â”€â”€ Shared text entity data (used by AttDef/AttEntity) â”€â”€â”€â”€â”€â”€â”€â”€

    /// Write the TEXT entity data structure shared by Text, AttDef, and AttEntity.
    /// This matches the C# `writeTextEntity` method.
    #[allow(clippy::too_many_arguments)]
    fn write_text_entity_data(
        &mut self,
        insertion_point: Vector3,
        alignment_point: Vector3,
        normal: Vector3,
        thickness: f64,
        oblique_angle: f64,
        rotation: f64,
        height: f64,
        width_factor: f64,
        text_value: &str,
        generation: i16,
        horizontal_alignment: i16,
        vertical_alignment: i16,
    ) {
        if self.version.r13_14_only() {
            // R13-R14: all fields present
            self.writer.write_bit_double(insertion_point.z); // elevation
            self.writer.write_raw_double(insertion_point.x);
            self.writer.write_raw_double(insertion_point.y);
            self.writer.write_raw_double(alignment_point.x);
            self.writer.write_raw_double(alignment_point.y);
            self.writer.write_3bit_double(normal);
            self.writer.write_bit_double(thickness);
            self.writer.write_bit_double(oblique_angle);
            self.writer.write_bit_double(rotation);
            self.writer.write_bit_double(height);
            self.writer.write_bit_double(width_factor);
            self.writer.write_variable_text(text_value);
            self.writer.write_bit_short(generation);
            self.writer.write_bit_short(horizontal_alignment);
            self.writer.write_bit_short(vertical_alignment);
        } else {
            // R2000+: DataFlags-based conditional encoding
            let mut data_flags: u8 = 0;
            if insertion_point.z == 0.0 {
                data_flags |= 0b0000_0001; // elevation is zero
            }
            if alignment_point == Vector3::ZERO {
                data_flags |= 0b0000_0010; // alignment point is zero
            }
            if oblique_angle == 0.0 {
                data_flags |= 0b0000_0100;
            }
            if rotation == 0.0 {
                data_flags |= 0b0000_1000;
            }
            if width_factor == 1.0 {
                data_flags |= 0b0001_0000;
            }
            if generation == 0 {
                data_flags |= 0b0010_0000; // no mirror
            }
            if horizontal_alignment == 0 {
                data_flags |= 0b0100_0000; // left
            }
            if vertical_alignment == 0 {
                data_flags |= 0b1000_0000; // baseline
            }

            self.writer.write_byte(data_flags);

            // Elevation RD â€” if !(flags & 0x01)
            if (data_flags & 0x01) == 0 {
                self.writer.write_raw_double(insertion_point.z);
            }
            // Insertion pt 2RD 10
            self.writer.write_raw_double(insertion_point.x);
            self.writer.write_raw_double(insertion_point.y);
            // Alignment pt 2DD 11 â€” if !(flags & 0x02)
            if (data_flags & 0x02) == 0 {
                self.writer
                    .write_bit_double_with_default(alignment_point.x, insertion_point.x);
                self.writer
                    .write_bit_double_with_default(alignment_point.y, insertion_point.y);
            }
            // Extrusion BE 210
            self.writer.write_bit_extrusion(normal);
            // Thickness BT 39
            self.writer.write_bit_thickness(thickness);
            // Oblique ang RD 51 â€” if !(flags & 0x04)
            if (data_flags & 0x04) == 0 {
                self.writer.write_raw_double(oblique_angle);
            }
            // Rotation ang RD 50 â€” if !(flags & 0x08)
            if (data_flags & 0x08) == 0 {
                self.writer.write_raw_double(rotation);
            }
            // Height RD 40
            self.writer.write_raw_double(height);
            // Width factor RD 41 â€” if !(flags & 0x10)
            if (data_flags & 0x10) == 0 {
                self.writer.write_raw_double(width_factor);
            }
            // Text value TV 1
            self.writer.write_variable_text(text_value);
            // Generation BS 71 â€” if !(flags & 0x20)
            if (data_flags & 0x20) == 0 {
                self.writer.write_bit_short(generation);
            }
            // Horiz align BS 72 â€” if !(flags & 0x40)
            if (data_flags & 0x40) == 0 {
                self.writer.write_bit_short(horizontal_alignment);
            }
            // Vert align BS 73 â€” if !(flags & 0x80)
            if (data_flags & 0x80) == 0 {
                self.writer.write_bit_short(vertical_alignment);
            }
        }
    }

    // â”€â”€ Legacy Polyline (2D) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_polyline_old(&mut self, e: &Polyline) {
        // Legacy Polyline â€” convert to Polyline3D for DWG output.
        // The DXF reader collapses Polyline2D/3D/PolyfaceMesh into this
        // legacy variant; we re-emit as Polyline3D so data isn't lost.
        let mut p3d = Polyline3D::new();
        p3d.common = e.common.clone();
        p3d.flags.closed = e.flags.is_closed();
        for v in &e.vertices {
            p3d.vertices.push(Vertex3DPolyline {
                handle: Handle::NULL,
                layer: e.common.layer.clone(),
                position: v.location,
                flags: v.flags.bits() as i32,
            });
        }
        self.write_polyline3d(&p3d);
    }

    // â”€â”€ ACIS entities (3DSOLID, REGION, BODY) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn write_solid3d(&mut self, e: &Solid3D) {
        self.entity_preamble(common::OBJ_3DSOLID, &e.common);

        let acds = self.needs_acds_section();
        if acds {
            // AC1027+: ACIS data is stored in the AcDsPrototype_1b section.
            // Entity stream writes acis_empty=true with no inline data.
            self.write_acis_empty();
            self.queue_sab_entry(&e.acis_data, e.common.handle);
        } else {
            self.write_acis_data(&e.acis_data, &e.wires, &e.silhouettes);
        }

        // acis_empty_bit â€” second copy of the "empty" flag.
        // Must match the acis_empty bit written above.
        self.writer.write_bit(acds);

        // R2007+: unknown BL field (COMMON_3DSOLID)
        if self.version.r2007_plus() {
            self.writer.write_bit_long(0);
        }

        // 3DSOLID R2007+: history_id handle
        if self.version.r2007_plus() {
            let h = e.history_handle.map(|h| h.value()).unwrap_or(0);
            self.writer.write_handle(DwgReferenceType::SoftPointer, h);
        }

        self.register_object(e.common.handle);
    }

    fn write_region(&mut self, e: &Region) {
        self.entity_preamble(common::OBJ_REGION, &e.common);

        let acds = self.needs_acds_section();
        if acds {
            self.write_acis_empty();
            self.queue_sab_entry(&e.acis_data, e.common.handle);
        } else {
            self.write_acis_data(&e.acis_data, &e.wires, &e.silhouettes);
        }

        // acis_empty_bit â€” must match acis_empty
        self.writer.write_bit(acds);

        // R2007+: unknown BL field (COMMON_3DSOLID)
        if self.version.r2007_plus() {
            self.writer.write_bit_long(0);
        }

        self.register_object(e.common.handle);
    }

    fn write_body(&mut self, e: &Body) {
        self.entity_preamble(common::OBJ_BODY, &e.common);

        let acds = self.needs_acds_section();
        if acds {
            self.write_acis_empty();
            self.queue_sab_entry(&e.acis_data, e.common.handle);
        } else {
            self.write_acis_data(&e.acis_data, &e.wires, &e.silhouettes);
        }

        // acis_empty_bit â€” must match acis_empty
        self.writer.write_bit(acds);

        // R2007+: unknown BL field (COMMON_3DSOLID)
        if self.version.r2007_plus() {
            self.writer.write_bit_long(0);
        }

        self.register_object(e.common.handle);
    }

    /// Write an empty ACIS entity stub (AC1027+).
    ///
    /// For R2013 and later, ACIS data lives in the AcDsPrototype_1b section.
    /// The entity stream simply indicates "no inline data":
    ///   acis_empty = true (B), wireframe_present = false (B).
    fn write_acis_empty(&mut self) {
        self.writer.write_bit(true); // acis_empty = true (no inline data)
        self.writer.write_bit(false); // wireframe_present = false
    }

    /// Queue SAB data for writing into the AcDsPrototype_1b section.
    ///
    /// Converts SAT text â†’ SAB binary if needed (mirroring the DXF writer's
    /// `queue_sab_data()` approach).
    fn queue_sab_entry(&mut self, acis: &AcisData, entity_handle: Handle) {
        if acis.is_binary && !acis.sab_data.is_empty() {
            // Already have SAB binary data
            self.sab_entries.push((entity_handle, acis.sab_data.clone()));
        } else if !acis.sat_data.is_empty() {
            // Convert SAT text â†’ SAB binary via SatDocument
            if let Ok(mut sat_doc) =
                crate::entities::acis::SatDocument::parse(&acis.sat_data)
            {
                sat_doc.strip_for_sab();
                let sab = crate::entities::acis::SabWriter::write(&sat_doc);
                self.sab_entries.push((entity_handle, sab));
            }
        }
    }

    /// Write ACIS/SAT modeler geometry data shared by 3DSOLID, REGION, BODY.
    ///
    /// This writes both `ENCODE_3DSOLID` (acis data) and the wireframe part
    /// of `COMMON_3DSOLID`.  The caller must still write `acis_empty_bit`
    /// and any version-dependent trailing fields (history_id, etc.).
    ///
    /// DWG entity streams always use SAT text (version 1) with the selective
    /// 159-cipher, regardless of the DWG file version.  SAB binary is a
    /// DXF-only concept stored in the ACDSDATA section â€” it is never written
    /// inline in the DWG entity stream.  If the entity contains SAB data,
    /// it is converted back to SAT text via [`SabReader`] + [`SatDocument`].
    fn write_acis_data(
        &mut self,
        acis: &AcisData,
        wires: &[Wire],
        silhouettes: &[Silhouette],
    ) {
        let has_data = acis.has_data();
        self.writer.write_bit(!has_data); // acis_empty (inverted: true = empty)

        if has_data {
            // Unknown bit â€” per ODA spec / LibreDWG / ACadSharp this B
            // is always present between acis_empty and the version BS.
            self.writer.write_bit(false);

            // DWG entity streams always use SAT text (version 1).
            // SAB binary (version 2) is only used in DXF ACDSDATA sections.
            self.writer.write_bit_short(1_i16);

            // Obtain SAT text â€” convert from SAB if needed.
            let sat_text = if !acis.sat_data.is_empty() {
                // Already have SAT text
                acis.sat_data.clone()
            } else if !acis.sab_data.is_empty() {
                // Convert SAB binary â†’ SAT text via SabReader + SatDocument
                match crate::entities::acis::SabReader::read(&acis.sab_data) {
                    Ok(sat_doc) => sat_doc.to_sat_string(),
                    Err(_) => String::new(),
                }
            } else {
                String::new()
            };

            // SAT text â€” all DWG versions use the same encoding:
            // BL-sized blocks of encrypted bytes (cipher: 159 - byte)
            // terminated by BL(0).  Per LibreDWG dwg.spec.
            let stripped = AcisData::strip_sat_terminator(&sat_text);
            let mut full = stripped.clone();
            full.push_str("End-of-ACIS-data\n");
            let plain = full.as_bytes();

            // Encrypt with selective 159-substitution cipher
            // (per LibreDWG dwg.spec: bytes <= 32 pass through, bytes > 32: 159 - byte)
            let mut encrypted = Vec::with_capacity(plain.len());
            for &b in plain.iter() {
                if b <= 32 {
                    encrypted.push(b);
                } else {
                    encrypted.push(159u8.wrapping_sub(b));
                }
            }

            // Write as a single block + terminating BL(0)
            self.writer.write_bit_long(encrypted.len() as i32);
            self.writer.write_bytes(&encrypted);
            self.writer.write_bit_long(0); // terminating empty block
        }

        // â”€â”€ COMMON_3DSOLID: Wireframe data (always present) â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let wireframe_present = has_data && !wires.is_empty();
        self.writer.write_bit(wireframe_present);

        if wireframe_present {
            // Point of reference (first wire's anchor â€” use first wire's first point or ZERO)
            let point = wires
                .first()
                .and_then(|w| w.points.first().copied())
                .unwrap_or(Vector3::ZERO);
            self.writer.write_3bit_double(point);

            self.writer.write_bit_long(wires.len() as i32);
            for wire in wires {
                self.write_wire(wire);
            }

            // Silhouettes (inside wireframe section per LibreDWG)
            self.writer.write_bit_long(silhouettes.len() as i32);
            for sil in silhouettes {
                self.writer.write_bit_long(sil.viewport_id as i32);
                self.writer.write_3bit_double(sil.view_direction);
                self.writer.write_3bit_double(sil.up_vector);
                self.writer.write_3bit_double(sil.target);
                self.writer.write_bit(sil.is_perspective);
                self.writer.write_bit_long(sil.wires.len() as i32);
                for wire in &sil.wires {
                    self.write_wire(wire);
                }
            }
        }

        // NOTE: acis_empty_bit is NOT written here â€” the caller writes it
        // after this function returns, along with any entity-specific data.
    }

    /// Write a single wire struct (shared by wires and silhouette wires).
    fn write_wire(&mut self, wire: &Wire) {
        self.writer.write_bit_long(wire.acis_index);
        self.writer.write_byte(wire.wire_type as u8);
        self.writer.write_bit_long(wire.selection_marker);
        let color_val: i32 = match wire.color {
            crate::types::Color::ByLayer => 256,
            crate::types::Color::ByBlock => 0,
            crate::types::Color::Index(idx) => idx as i32,
            _ => 256,
        };
        self.writer.write_bit_long(color_val);
        self.writer.write_bit_long(wire.points.len() as i32);
        for pt in &wire.points {
            self.writer.write_3bit_double(*pt);
        }
        self.writer.write_bit(wire.has_transform);
        if wire.has_transform {
            self.writer.write_3bit_double(wire.x_axis);
            self.writer.write_3bit_double(wire.y_axis);
            self.writer.write_3bit_double(wire.z_axis);
            self.writer.write_3bit_double(wire.translation);
            self.writer.write_bit_double(wire.scale);
            self.writer.write_bit(wire.has_rotation);
            self.writer.write_bit(wire.has_reflection);
            self.writer.write_bit(wire.has_shear);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::CadDocument;
    use crate::entities::{EntityCommon, Line, Point};
    use crate::types::{Handle, Vector3};

    fn make_doc_with_entity(entity: EntityType) -> CadDocument {
        let mut doc = CadDocument::new();
        let handle = entity.common().handle;
        let idx = doc.entities.len();
        doc.entities.push(entity);
        doc.entity_index.insert(handle, idx);
        if let Some(br) = doc.block_records.get_mut("*Model_Space") {
            br.entity_handles.push(handle);
        }
        doc
    }

    #[test]
    fn write_point_entity() {
        let pt = Point {
            common: EntityCommon {
                handle: Handle::new(0x100),
                ..Default::default()
            },
            location: Vector3::new(1.0, 2.0, 3.0),
            thickness: 0.0,
            normal: Vector3::UNIT_Z,
        };
        let doc = make_doc_with_entity(EntityType::Point(pt));
        let writer = DwgObjectWriter::new(&doc).unwrap();
        let (output, _map, _, _) = writer.write();
        assert!(!output.is_empty());
    }

    #[test]
    fn write_line_entity() {
        let line = Line {
            common: EntityCommon {
                handle: Handle::new(0x101),
                ..Default::default()
            },
            start: Vector3::new(0.0, 0.0, 0.0),
            end: Vector3::new(10.0, 20.0, 0.0),
            thickness: 0.0,
            normal: Vector3::UNIT_Z,
        };
        let doc = make_doc_with_entity(EntityType::Line(line));
        let writer = DwgObjectWriter::new(&doc).unwrap();
        let (output, _map, _, _) = writer.write();
        assert!(!output.is_empty());
    }
}
