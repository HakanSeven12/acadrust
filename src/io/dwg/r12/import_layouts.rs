use crate::{objects::ObjectType, CadDocument};

/// Defaults for the modern layouts synthesized by the R12 DXF bridge.
/// Independent AC1009 imports in ACAD 2024, 2025 and 2027 agree on these
/// settings. Printer selection depends on the importing application profile
/// and deliberately remains outside the native decoder.
pub(super) fn apply_defaults(document: &mut CadDocument) {
    for object in document.objects.values_mut() {
        let ObjectType::Layout(layout) = object else {
            continue;
        };
        let model = match layout.name.as_str() {
            "Model" => true,
            "Layout1" => false,
            _ => continue,
        };
        layout.plot_flags.use_standard_scale = true;
        layout.plot_flags.plot_plot_styles = true;
        layout.plot_flags.print_lineweights = true;
        layout.plot_flags.draw_viewports_first = true;
        layout.plot_flags.model_type = model;
        layout.plot_type = if model { 0 } else { 5 }; // Display / Layout
        layout.plot_scale_type = if model { 0 } else { 16 }; // Fit / 1:1
        layout.shade_plot_resolution = 2; // Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::dwg::r12::import_handles_tests::{fixture, record};
    use crate::io::dwg::r12::EntityRecord;
    use crate::DwgReader;
    use std::io::Cursor;

    fn assert_imported_layout_settings(document: &CadDocument) {
        let mut names = Vec::new();
        for object in document.objects.values() {
            let ObjectType::Layout(layout) = object else {
                continue;
            };
            names.push(layout.name.as_str());
            let (flags, area, scale) = match layout.name.as_str() {
                "Model" => (1712, 0, 0),
                "Layout1" => (688, 5, 16),
                name => panic!("Unexpected imported layout {name}"),
            };
            assert_eq!(layout.plot_flags.to_bits(), flags);
            assert_eq!(layout.plot_type, area);
            assert_eq!(layout.plot_scale_type, scale);
            assert_eq!(layout.shade_plot_resolution, 2);
            assert_eq!(layout.shade_plot_dpi, 300);
            assert!(layout.plot_printer_name.is_empty());
            assert!(layout.paper_size.is_empty());
            assert_eq!(layout.paper_width, 0.0);
            assert_eq!(layout.paper_height, 0.0);
            assert_eq!(layout.plot_scale_numerator, 1.0);
            assert_eq!(layout.plot_scale_denominator, 1.0);
        }
        names.sort();
        assert_eq!(names, ["Layout1", "Model"]);
    }

    #[test]
    fn synthesized_layout_settings_match_independent_imports() {
        let mut tables: [Vec<EntityRecord>; 10] = std::array::from_fn(|_| Vec::new());
        tables[1] = vec![record("LAYER", &[(2, "0")])];
        tables[2] = vec![record("STYLE", &[(2, "STANDARD"), (3, "txt")])];
        tables[3] = vec![record("LTYPE", &[(2, "CONTINUOUS")])];
        // Both disabled and enabled handle storage must get identical defaults.
        for seed in [0, 0x100] {
            let document = DwgReader::from_stream(Cursor::new(fixture(seed, &tables, &[], &[])))
                .read()
                .unwrap();
            assert_imported_layout_settings(&document);
        }
    }
}
