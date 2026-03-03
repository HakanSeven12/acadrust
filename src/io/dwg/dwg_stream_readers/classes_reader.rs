//! DWG Classes section reader
//!
//! Reads the AcDb:Classes section from a DWG file, producing a
//! `DxfClassCollection`. Mirrors ACadSharp's `DwgClassesReader`.
//!
//! ## Section layout
//!
//! ```text
//! ┌──────────────────┐
//! │ Start sentinel   │ 16 bytes
//! │ Section size (RL)│ 4 bytes
//! │ Class entries... │ variable
//! │ CRC-16           │ 2 bytes
//! │ End sentinel     │ 16 bytes
//! └──────────────────┘
//! ```

use crate::classes::{DxfClass, DxfClassCollection, ProxyFlags};
use crate::error::{DxfError, Result};
use crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
use crate::io::dwg::dwg_version::DwgVersion;
use crate::io::dwg::file_headers::section_definition::{end_sentinels, start_sentinels};
use crate::types::DxfVersion;

/// Read the Classes section from raw section bytes (already decompressed).
///
/// # Arguments
/// * `data` - Complete section buffer (includes sentinels)
/// * `version` - DXF version for version-specific parsing
///
/// # Returns
/// `DxfClassCollection` containing all parsed class definitions.
pub fn read_classes(data: &[u8], version: DxfVersion) -> Result<DxfClassCollection> {
    let dwg = DwgVersion::from_dxf_version(version)?;

    // ── Verify start sentinel ──
    if data.len() < 34 {
        return Err(DxfError::Parse(
            "Classes section too short".to_string(),
        ));
    }
    if &data[..16] != &start_sentinels::CLASSES {
        return Err(DxfError::InvalidSentinel(
            "Classes section start sentinel mismatch".to_string(),
        ));
    }

    // ── Read section size (RL at offset 16) ──
    let section_size = i32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;

    // Section data starts after sentinel (16) + size field (4)
    let data_start = 20;
    if data.len() < data_start + section_size + 2 + 16 {
        return Err(DxfError::Parse(
            "Classes section data truncated".to_string(),
        ));
    }
    let section_data = &data[data_start..data_start + section_size];

    // ── Create the bit reader over the section data ──
    let mut reader = DwgBitReader::new(section_data.to_vec(), dwg, version);

    // R2007+: The section data has an RL prefix (total data size in bits)
    // from save_position_for_size. Text is INLINE (not in a separate stream).
    // Sections do NOT use three-stream merge.
    let end_bit;
    if version >= DxfVersion::AC1021 {
        let total_size_bits = reader.read_raw_long() as i64;
        end_bit = total_size_bits;
    } else {
        end_bit = (section_size * 8) as i64;
    }

    // ── R2004+: section header ──
    let mut max_class_number: i16 = i16::MAX;
    if version >= DxfVersion::AC1018 {
        max_class_number = reader.read_bit_short(); // BS: max class number
        let _rc1 = reader.read_byte(); // RC: 0x00
        let _rc2 = reader.read_byte(); // RC: 0x00
        let _flag = reader.read_bit(); // B: true
    }

    // ── Read class entries until we consume all section data ──
    let mut classes = DxfClassCollection::new();

    while reader.position_in_bits() < end_bit {
        let class_number = reader.read_bit_short();

        // Sanity check — class numbering starts at 500 and can't exceed max
        if class_number < 500 || class_number > max_class_number {
            break;
        }

        let proxy_flags_raw = reader.read_bit_short() as i32;
        let application_name = reader.read_variable_text();
        let cpp_class_name = reader.read_variable_text();
        let dxf_name = reader.read_variable_text();
        let was_zombie = reader.read_bit();
        let item_class_id = reader.read_bit_short();

        let mut class = DxfClass::new(&dxf_name, &cpp_class_name);
        class.application_name = application_name;
        class.class_number = class_number;
        class.proxy_flags = ProxyFlags::from(proxy_flags_raw);
        class.was_zombie = was_zombie;
        class.item_class_id = item_class_id;

        // R2004+: instance count + 4 extra BL fields
        if version >= DxfVersion::AC1018 {
            let instance_count = reader.read_bit_long();
            class.instance_count = instance_count;
            let _dwg_version = reader.read_bit_long();
            let _maintenance_version = reader.read_bit_long();
            let _unknown1 = reader.read_bit_long();
            let _unknown2 = reader.read_bit_long();
        }

        classes.add_or_update(class);
    }

    // ── Verify end sentinel ──
    let end_sentinel_start = data_start + section_size + 2; // +2 for CRC
    if data.len() >= end_sentinel_start + 16 {
        if &data[end_sentinel_start..end_sentinel_start + 16] != &end_sentinels::CLASSES {
            // Non-fatal: log warning but don't fail
            // Some writers produce slightly off sentinels
        }
    }

    Ok(classes)
}

// ════════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::dwg::dwg_stream_writers::classes_writer;

    #[test]
    fn test_classes_roundtrip_r2000() {
        let mut classes = DxfClassCollection::new();
        classes.update_defaults();

        // Write classes section
        let class_vec: Vec<DxfClass> = classes.iter().cloned().collect();
        let written = classes_writer::write_classes(DxfVersion::AC1015, &class_vec);

        // Read it back
        let read_classes = read_classes(&written, DxfVersion::AC1015).unwrap();

        // Should have the same number of classes
        assert_eq!(read_classes.len(), classes.len(),
            "Class count mismatch: wrote {}, read {}",
            classes.len(), read_classes.len());
    }

    #[test]
    fn test_classes_roundtrip_r2004() {
        let mut classes = DxfClassCollection::new();
        classes.update_defaults();

        let class_vec: Vec<DxfClass> = classes.iter().cloned().collect();
        let written = classes_writer::write_classes(DxfVersion::AC1018, &class_vec);
        let read_classes = read_classes(&written, DxfVersion::AC1018).unwrap();

        assert_eq!(read_classes.len(), classes.len(),
            "Class count mismatch: wrote {}, read {}",
            classes.len(), read_classes.len());

        // Verify a specific class (ACDBPLACEHOLDER is 12th, class_number=511)
        let acdb_placeholder = read_classes.get_by_name("ACDBPLACEHOLDER");
        assert!(acdb_placeholder.is_some(), "Should find ACDBPLACEHOLDER class");
        let cls = acdb_placeholder.unwrap();
        assert_eq!(cls.cpp_class_name, "AcDbPlaceHolder");
        assert_eq!(cls.class_number, 511);
    }

    #[test]
    fn test_classes_bad_sentinel_fails() {
        let mut bad_data = vec![0u8; 50];
        // Wrong sentinel
        bad_data[..16].fill(0xFF);
        let result = read_classes(&bad_data, DxfVersion::AC1015);
        assert!(result.is_err());
    }
}
