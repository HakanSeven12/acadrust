//! DWG Preview (thumbnail image) section writer
//!
//! Writes the preview section for DWG files. For now, writes an empty
//! preview (no thumbnail), which is the simplest valid form.
//!
//! ## Format
//!
//! ```text
//! [Start sentinel: 16 bytes]
//! [Overall size: RL (4 bytes)]
//! [Images present: RC (1 byte, 0 = none)]
//! [End sentinel: 16 bytes]
//! ```
//!
//! Based on ACadSharp's `DwgPreviewWriter`.

use crate::io::dwg::dwg_stream_writers::DwgBitWriter;
use crate::io::dwg::dwg_version::DwgVersion;
use crate::io::dwg::file_headers::section_definition::{start_sentinels, end_sentinels};
use crate::types::DxfVersion;

/// Write an empty preview section (no thumbnail).
///
/// This is the minimal valid preview — just sentinels wrapping size=1
/// and count=0. AutoCAD will display no thumbnail but will accept the file.
///
/// # Returns
/// The complete preview section bytes, ready for `add_section("AcDb:Preview", ...)`.
pub fn write_preview(version: DxfVersion) -> Vec<u8> {
    let dwg_version = DwgVersion::from_dxf_version(version)
        .unwrap_or(DwgVersion::AC15);
    let mut writer = DwgBitWriter::new(dwg_version, version);

    // Start sentinel
    writer.write_bytes(&start_sentinels::PREVIEW);

    // Overall size of image area (RL = raw long)
    writer.write_raw_long(1);

    // Images present (RC = raw byte): 0 = none
    writer.write_byte(0);

    // End sentinel
    writer.write_bytes(&end_sentinels::PREVIEW);

    writer.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_empty_preview() {
        let data = write_preview(DxfVersion::AC1015);

        // Should be: 16 (start sentinel) + 4 (size) + 1 (count) + 16 (end sentinel) = 37 bytes
        assert_eq!(data.len(), 37);

        // Start sentinel
        assert_eq!(&data[0..16], &start_sentinels::PREVIEW);

        // Size = 1 (LE)
        assert_eq!(&data[16..20], &[1, 0, 0, 0]);

        // Images present = 0
        assert_eq!(data[20], 0);

        // End sentinel
        assert_eq!(&data[21..37], &end_sentinels::PREVIEW);
    }

    #[test]
    fn test_preview_sentinels_are_complement() {
        for i in 0..16 {
            assert_eq!(
                start_sentinels::PREVIEW[i] ^ end_sentinels::PREVIEW[i],
                0xFF,
            );
        }
    }
}
