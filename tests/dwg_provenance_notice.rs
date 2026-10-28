const NOTICES: &str = include_str!("../THIRD_PARTY_NOTICES.md");

#[test]
fn includes_acadsharp_mit_attribution_and_license_terms() {
    for required in [
        "ACadSharp",
        "Copyright (c) 2021 Albert Domenech",
        "Permission is hereby granted, free of charge",
        "The above copyright notice and this permission notice",
        "THE SOFTWARE IS PROVIDED \"AS IS\"",
    ] {
        assert!(
            NOTICES.contains(required),
            "missing notice text: {required}"
        );
    }
}

#[test]
fn records_oda_and_native_cad_files_as_format_evidence() {
    for required in [
        "Open Design Alliance (ODA) DWG specification",
        "native DWG files produced by CAD",
        "ACadSharp remains the attributed origin",
    ] {
        assert!(
            NOTICES.contains(required),
            "missing provenance text: {required}"
        );
    }
}
