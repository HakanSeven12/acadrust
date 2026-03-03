//! DWG Classes section writer
//!
//! Writes the CLASSES section containing DXF class definitions.
//! Each class maps a DXF entity/object name to its C++ class name and
//! the application that registered it.
//!
//! ## Section format
//!
//! ```text
//! start_sentinel (16 bytes)
//! ┌─ CRC-16 scope ──────────────────────────┐
//! │ RL  size of class data area              │
//! │ [RL 0x00000000  — R2010+/R2013+ only]    │
//! │ <class data bytes>                       │
//! └──────────────────────────────────────────┘
//! RS  CRC-16
//! end_sentinel (16 bytes)
//! [8 zero bytes — R2004+ only]
//! ```
//!
//! Based on ACadSharp's `DwgClassesWriter`.

use crate::classes::DxfClass;
use crate::io::dwg::crc::{crc16, CRC16_SEED};
use crate::io::dwg::dwg_stream_writers::DwgMergedWriter;
use crate::io::dwg::dwg_version::DwgVersion;
use crate::io::dwg::file_headers::section_definition::{end_sentinels, start_sentinels};
use crate::types::DxfVersion;

/// Write the complete Classes section.
///
/// # Arguments
/// * `version` - Target DXF/DWG version
/// * `classes` - Slice of DXF class definitions to write
///
/// # Returns
/// Complete section bytes including sentinels and CRC.
pub fn write_classes(version: DxfVersion, classes: &[DxfClass]) -> Vec<u8> {
    let dwg_version =
        DwgVersion::from_dxf_version(version).unwrap_or(DwgVersion::AC15);

    // R2007+: Use DwgMergedWriter with three-stream merge.
    // Text (class names via WriteVariableText) goes to the text sub-stream,
    // which is merged into the final output with text-size flag words.
    // This matches C# ACadSharp's DwgClassesWriter which uses
    // DwgMergedStreamWriter for R2007+.
    // Pre-R2007: Use DwgMergedWriter in two-stream mode (text = main).
    if version >= DxfVersion::AC1021 {
        let mut writer = DwgMergedWriter::new(dwg_version, version);

        // Save position for the size placeholder (4-byte RL = total data bits)
        writer.save_position_for_size();

        // Section header (R2004+)
        let max_class_number = classes
            .iter()
            .map(|c| c.class_number)
            .max()
            .unwrap_or(0);
        writer.write_bit_short(max_class_number);
        writer.write_byte(0);
        writer.write_byte(0);
        writer.write_bit(true);

        // Write each class definition.
        // write_variable_text routes to the text sub-stream for R2007+.
        for c in classes {
            writer.write_bit_short(c.class_number);
            writer.write_bit_short(c.proxy_flags.0 as i16);
            writer.write_variable_text(&c.application_name);
            writer.write_variable_text(&c.cpp_class_name);
            writer.write_variable_text(&c.dxf_name);
            writer.write_bit(c.was_zombie);
            writer.write_bit_short(c.item_class_id);
            writer.write_bit_long(c.instance_count);
            writer.write_bit_long(0);
            writer.write_bit_long(0);
            writer.write_bit_long(0);
            writer.write_bit_long(0);
        }

        // merge() handles: RL patching, text-size flags, byte alignment
        let section_data = writer.merge();
        write_size_and_crc(version, &section_data)
    } else {
        // Pre-R2007: use DwgMergedWriter (two-stream mode, text inline)
        let mut writer = DwgMergedWriter::new(dwg_version, version);

        // R2004+: section header
        if version >= DxfVersion::AC1018 {
            let max_class_number = classes
                .iter()
                .map(|c| c.class_number)
                .max()
                .unwrap_or(0);
            writer.write_bit_short(max_class_number);
            writer.write_byte(0);
            writer.write_byte(0);
            writer.write_bit(true);
        }

        // Write each class definition
        for c in classes {
            writer.write_bit_short(c.class_number);
            writer.write_bit_short(c.proxy_flags.0 as i16);
            writer.write_variable_text(&c.application_name);
            writer.write_variable_text(&c.cpp_class_name);
            writer.write_variable_text(&c.dxf_name);
            writer.write_bit(c.was_zombie);
            writer.write_bit_short(c.item_class_id);

            if version >= DxfVersion::AC1018 {
                writer.write_bit_long(c.instance_count);
                writer.write_bit_long(0);
                writer.write_bit_long(0);
                writer.write_bit_long(0);
                writer.write_bit_long(0);
            }
        }

        let section_data = writer.merge();
        write_size_and_crc(version, &section_data)
    }
}

/// Wrap section data with sentinels, size, and CRC-16.
///
/// This implements the `writeSizeAndCrc()` pattern from C#.
fn write_size_and_crc(version: DxfVersion, section_data: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(
        16 + 4 + section_data.len() + 2 + 16 + 8,
    );

    // Start sentinel (16 bytes)
    output.extend_from_slice(&start_sentinels::CLASSES);

    // CRC-16 covers: size field(s) + section data
    let mut crc_content = Vec::with_capacity(4 + section_data.len());

    // RL: size of class data area
    crc_content.extend_from_slice(&(section_data.len() as i32).to_le_bytes());

    // R2010+ (AC1024 maint>3) or R2013+ (AC1032): extra 4 zero bytes
    // Note: the full C# condition is:
    //   (version >= AC1024 && maintenanceVersion > 3) || version > AC1027
    // We use the simplified check (AC1032+) since maintenance version is
    // not tracked separately in our HeaderVariables.
    if version > DxfVersion::AC1027 {
        crc_content.extend_from_slice(&0i32.to_le_bytes());
    }

    // Section data bytes
    crc_content.extend_from_slice(section_data);

    // Compute CRC-16 over the content
    let crc = crc16(CRC16_SEED, &crc_content);

    // Write CRC-wrapped content + CRC value
    output.extend_from_slice(&crc_content);
    output.extend_from_slice(&crc.to_le_bytes());

    // End sentinel (16 bytes)
    output.extend_from_slice(&end_sentinels::CLASSES);

    // R2004+: 8 trailing zero bytes
    if version >= DxfVersion::AC1018 {
        output.extend_from_slice(&[0u8; 8]);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::ProxyFlags;

    fn make_test_class(number: i16, name: &str) -> DxfClass {
        DxfClass {
            dxf_name: name.to_string(),
            cpp_class_name: format!("AcDb{}", name),
            application_name: "ObjectDBX Classes".to_string(),
            proxy_flags: ProxyFlags::NONE,
            instance_count: 0,
            was_zombie: false,
            is_an_entity: false,
            class_number: number,
            item_class_id: 0x1F3, // object
        }
    }

    #[test]
    fn test_write_classes_empty() {
        let data = write_classes(DxfVersion::AC1015, &[]);

        // Should have start sentinel (16) + size(4) + section data + CRC(2) + end sentinel (16)
        assert!(data.len() >= 16 + 4 + 2 + 16);

        // Start sentinel
        assert_eq!(&data[..16], &start_sentinels::CLASSES);
        // End sentinel (before final position)
        let end_start = data.len() - 16;
        assert_eq!(&data[end_start..], &end_sentinels::CLASSES);
    }

    #[test]
    fn test_write_classes_r2004_has_trailing_zeros() {
        let data = write_classes(DxfVersion::AC1018, &[]);

        // R2004+: should end with 8 zero bytes after end sentinel
        let last8 = &data[data.len() - 8..];
        assert_eq!(last8, &[0u8; 8]);
    }

    #[test]
    fn test_write_classes_with_one_class() {
        let cls = make_test_class(500, "PLACEHOLDER");
        let data = write_classes(DxfVersion::AC1015, &[cls]);

        // Should have sentinels
        assert_eq!(&data[..16], &start_sentinels::CLASSES);

        // Size field at offset 16 (4 bytes LE)
        let size = i32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        assert!(size > 0, "Section data size should be > 0");
    }

    #[test]
    fn test_write_classes_r2004_header() {
        let cls = make_test_class(500, "TEST");
        let data = write_classes(DxfVersion::AC1018, &[cls.clone()]);

        // Should be longer than R2000 version (extra per-class fields + section header)
        let data_r2000 = write_classes(DxfVersion::AC1015, &[cls]);
        assert!(
            data.len() > data_r2000.len(),
            "R2004 classes should be longer than R2000: {} vs {}",
            data.len(),
            data_r2000.len()
        );
    }

    #[test]
    fn test_write_classes_crc_present() {
        let cls = make_test_class(500, "X");
        let data = write_classes(DxfVersion::AC1015, &[cls]);

        // CRC is 2 bytes before end sentinel
        let end_sentinel_start = data.len() - 16;
        let crc_bytes = &data[end_sentinel_start - 2..end_sentinel_start];

        // Re-compute CRC over (size + section_data) and verify
        let size_plus_data = &data[16..end_sentinel_start - 2];
        let expected_crc = crc16(CRC16_SEED, size_plus_data);
        let actual_crc = u16::from_le_bytes([crc_bytes[0], crc_bytes[1]]);
        assert_eq!(
            actual_crc, expected_crc,
            "CRC mismatch: got 0x{:04X}, expected 0x{:04X}",
            actual_crc, expected_crc
        );
    }
}
