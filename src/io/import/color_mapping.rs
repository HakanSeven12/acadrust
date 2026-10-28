//! RGB-to-ACAD color mapping and material-layer utilities.

use crate::document::CadDocument;
use crate::tables::{Layer, TableEntry};
use crate::types::Color;

/// Map an RGB colour to the nearest ACAD Color Index (ACI 1–255).
///
/// Uses Euclidean distance in RGB space.  If you need true-colour fidelity,
/// use [`Color::Rgb`] directly instead.
pub fn rgb_to_aci(r: u8, g: u8, b: u8) -> Color {
    Color::Index(crate::types::aci_table::nearest_aci(r, g, b))
}

/// Round a 0–1 channel to an 8-bit value. Non-finite and out-of-range inputs saturate.
pub fn unit_channel_to_u8(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// True-colour value from 0–1 channels, rounded to the nearest byte.
pub fn color_from_unit_rgb(r: f32, g: f32, b: f32) -> Color {
    Color::Rgb {
        r: unit_channel_to_u8(r),
        g: unit_channel_to_u8(g),
        b: unit_channel_to_u8(b),
    }
}

/// Map an RGB colour to a [`Color`], preferring true colour (`Rgb`) when the
/// DXF version supports it (R2004+), otherwise falling back to the nearest ACI.
pub fn rgb_to_color(r: u8, g: u8, b: u8, use_true_color: bool) -> Color {
    if use_true_color {
        Color::Rgb { r, g, b }
    } else {
        rgb_to_aci(r, g, b)
    }
}

/// Create a named layer in the document with the given colour.
///
/// If a layer with the same name already exists it is left unchanged and the
/// existing name is returned.  The `prefix` is prepended with an underscore
/// separator unless it is empty.
pub fn create_material_layer(
    doc: &mut CadDocument,
    prefix: &str,
    material_name: &str,
    color: Color,
) -> String {
    let layer_name = if prefix.is_empty() {
        sanitize_layer_name(material_name)
    } else {
        format!("{}_{}", prefix, sanitize_layer_name(material_name))
    };

    if doc.layers.get(&layer_name).is_none() {
        let mut layer = Layer::with_color(&layer_name, color);
        layer.set_handle(doc.allocate_handle());
        // Ignore error if layer already exists (race with case-insensitive check)
        let _ = doc.layers.add(layer);
    }

    layer_name
}

/// Sanitize a string for use as a DXF layer name.
///
/// Replaces characters that are invalid in ACAD layer names with `_`.
fn sanitize_layer_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '<' | '>' | '/' | '\\' | '"' | ':' | ';' | '?' | '*' | '|' | ',' | '=' | '`' => {
                out.push('_');
            }
            c if c.is_ascii_control() => {
                out.push('_');
            }
            c => out.push(c),
        }
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_aci_exact_red() {
        assert_eq!(rgb_to_aci(255, 0, 0), Color::Index(1));
    }

    #[test]
    fn test_rgb_to_aci_exact_yellow() {
        assert_eq!(rgb_to_aci(255, 255, 0), Color::Index(2));
    }

    #[test]
    fn test_rgb_to_aci_exact_white() {
        assert_eq!(rgb_to_aci(255, 255, 255), Color::Index(7));
    }

    #[test]
    fn test_rgb_to_aci_near_red() {
        // Should map to red (index 1)
        assert_eq!(rgb_to_aci(250, 5, 5), Color::Index(1));
    }

    #[test]
    fn test_sanitize_layer_name() {
        assert_eq!(sanitize_layer_name("Metal/Chrome"), "Metal_Chrome");
        assert_eq!(sanitize_layer_name("wood:oak"), "wood_oak");
        assert_eq!(sanitize_layer_name(""), "unnamed");
    }

    #[test]
    fn test_rgb_to_color_true_color() {
        assert_eq!(
            rgb_to_color(123, 45, 67, true),
            Color::Rgb {
                r: 123,
                g: 45,
                b: 67
            }
        );
    }

    #[test]
    fn test_rgb_to_color_aci_fallback() {
        let c = rgb_to_color(255, 0, 0, false);
        assert_eq!(c, Color::Index(1));
    }

    #[test]
    fn test_unit_channel_rounds_half_up() {
        assert_eq!(unit_channel_to_u8(0.5), 128);
        assert_eq!(unit_channel_to_u8(1.0), 255);
        assert_eq!(unit_channel_to_u8(2.0), 255);
        assert_eq!(unit_channel_to_u8(f32::NAN), 0);
    }
}
