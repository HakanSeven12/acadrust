use super::super::transform;
use super::*;
use crate::types::Matrix3;

pub(crate) fn mirror_insert(e: &mut Insert, transform: &Transform) {
    mirror_insert_with_options(e, transform, MirrorOptions::default());
}

pub(crate) fn mirror_insert_with_options(
    e: &mut Insert,
    transform: &Transform,
    options: MirrorOptions,
) {
    if !valid_transform(transform) {
        return;
    }
    let old_frame = Matrix3::arbitrary_axis(e.normal) * Matrix3::rotation_z(e.rotation);
    let column = transform.apply_rotation(old_frame * Vector3::new(e.column_spacing, 0.0, 0.0));
    let row = transform.apply_rotation(old_frame * Vector3::new(0.0, e.row_spacing, 0.0));
    let mut attributes = std::mem::take(&mut e.attributes);
    transform::transform_insert(e, transform);
    if transform::is_reflecting(transform) {
        // ACAD keeps the Y scale sign and records reflection in X.
        e.rotation = (e.rotation + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU);
        e.set_x_scale(-e.x_scale());
        e.set_y_scale(-e.y_scale());
    }
    let new_frame = Matrix3::arbitrary_axis(e.normal) * Matrix3::rotation_z(e.rotation);
    // Spacings are signed offsets in the unscaled insertion frame. A
    // reflected array reverses its local Y direction, including negative
    // source spacings; taking vector lengths loses that information.
    e.column_spacing = (new_frame.transpose() * column).x;
    e.row_spacing = (new_frame.transpose() * row).y;
    for attribute in &mut attributes {
        mirror_attribute_entity_with_options(attribute, transform, options);
    }
    e.attributes = attributes;
    e.common.raw_record = None;
}

pub(crate) fn mirror_underlay(e: &mut Underlay, transform: &Transform) {
    if !valid_transform(transform) {
        return;
    }
    let old_frame = Matrix3::arbitrary_axis(e.normal) * Matrix3::rotation_z(e.rotation);
    let x = transform.apply_rotation(old_frame * Vector3::new(e.x_scale, 0.0, 0.0));
    let y = transform.apply_rotation(old_frame * Vector3::new(0.0, e.y_scale, 0.0));
    let z = transform.apply_rotation(old_frame * Vector3::new(0.0, 0.0, e.z_scale));
    transform::transform_underlay(e, transform);
    let new_frame = Matrix3::arbitrary_axis(e.normal) * Matrix3::rotation_z(e.rotation);
    let new_local = new_frame.transpose();
    e.x_scale = (new_local * x).x;
    e.y_scale = (new_local * y).y;
    e.z_scale = (new_local * z).z;
    e.common.raw_record = None;
}
