use super::super::transform;
use super::*;

pub(crate) fn mirror_hatch(e: &mut Hatch, transform: &Transform) {
    mirror_hatch_with_options(e, transform, MirrorOptions::default());
}

pub(crate) fn mirror_hatch_with_options(
    e: &mut Hatch,
    transform: &Transform,
    options: MirrorOptions,
) {
    if !valid_transform(transform) {
        return;
    }
    let reflecting = transform::is_reflecting(transform);
    let old_origin = e.pattern_origin();
    let old_extended_data = e.common.extended_data.clone();
    let pattern_angle = e.pattern_angle;
    let preserved_pattern = (reflecting && !options.mirror_hatch).then(|| {
        (
            e.pattern.clone(),
            e.pattern_scale,
            e.gradient_color.angle,
            e.common.extended_data.clone(),
        )
    });
    transform::transform_hatch(e, transform);
    if old_origin.length() < 1e-12 && e.pattern_origin().length() < 1e-12 {
        e.common.extended_data = old_extended_data;
    }
    if reflecting {
        // ACAD retains the authored angle while MIRRHATCH controls the
        // explicit line families. Their transformed angles carry reflection.
        e.pattern_angle = pattern_angle;
    }
    if let Some((pattern, scale, gradient_angle, extended_data)) = preserved_pattern {
        e.pattern = pattern;
        e.pattern_scale = scale;
        e.gradient_color.angle = gradient_angle;
        e.common.extended_data = extended_data;
    }
    e.common.raw_record = None;
}
