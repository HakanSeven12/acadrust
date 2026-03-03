//! DWG Object Reader — Core Framework
//!
//! Reads the AcDb:AcDbObjects section, parsing each object record
//! from the handle map. Each record has the framing:
//!
//! ```text
//! [ModularShort(size)][R2010+: ModularChar(handle_bits)][merged_data][CRC16]
//! ```
//!
//! The reader dispatches by type code to specific entity/table/object
//! readers (implemented in sibling modules in later phases).
//!
//! Based on ACadSharp's `DwgObjectReader.cs`.

pub mod common;
pub mod tables;
pub mod entities;
pub mod objects;

use std::collections::HashMap;
use crate::error::{DxfError, Result};
use crate::io::dwg::dwg_version::DwgVersion;
use crate::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use crate::types::{DxfVersion, Color, Transparency};

/// Maximum number of items in any array read from the stream.
/// Prevents infinite loops when reading corrupt data.
const MAX_ARRAY_COUNT: i32 = 100_000;

/// Cap a loop count read from the stream to a safe maximum.
#[inline]
fn safe_count(raw: i32) -> i32 {
    raw.max(0).min(MAX_ARRAY_COUNT)
}

/// Result of reading a single object record's common data.
#[derive(Debug, Clone)]
pub struct ObjectCommonData {
    /// DWG type code (fixed 0–82 or class number 500+)
    pub type_code: i16,
    /// Object handle
    pub handle: u64,
    /// Extended data (currently skipped, placeholder)
    pub xdata_size: usize,
}

/// Common entity data read from the entity preamble.
#[derive(Debug, Clone)]
pub struct EntityCommonData {
    /// Base common data (type + handle + xdata)
    pub common: ObjectCommonData,
    /// Has graphic data flag
    pub has_graphic: bool,
    /// Entity mode (0=owned, 1=paper, 2=model, 3=unused)
    pub entity_mode: u8,
    /// Owner handle (only if entity_mode == 0)
    pub owner_handle: u64,
    /// Reactor handles
    pub reactors: Vec<u64>,
    /// XDictionary handle (if present)
    pub xdictionary_handle: Option<u64>,
    /// Color
    pub color: Color,
    /// Transparency
    pub transparency: Transparency,
    /// Line weight (raw byte)
    pub line_weight: u8,
    /// Linetype scale
    pub linetype_scale: f64,
    /// Invisibility flag
    pub invisible: bool,
    /// Layer handle
    pub layer_handle: u64,
    /// Linetype flags (00=bylayer, 01=byblock, 10=continuous, 11=handle)
    pub linetype_flags: u8,
    /// Previous entity handle (pre-R2004)
    pub prev_entity_handle: Option<u64>,
    /// Next entity handle (pre-R2004)
    pub next_entity_handle: Option<u64>,
}

/// Common non-entity object data.
#[derive(Debug, Clone)]
pub struct NonEntityCommonData {
    /// Base common data (type + handle + xdata)
    pub common: ObjectCommonData,
    /// Owner handle
    pub owner_handle: u64,
    /// Reactor handles
    pub reactors: Vec<u64>,
    /// XDictionary handle (if present)
    pub xdictionary_handle: Option<u64>,
}

/// DWG Object Reader — iterates the object section by handle map.
pub struct DwgObjectReader {
    /// Raw object section data
    data: Vec<u8>,
    /// DWG version
    version: DwgVersion,
    /// DXF version
    dxf_version: DxfVersion,
    /// Handle → byte-offset map (from handle section)
    handle_map: HashMap<u64, i64>,
}

impl DwgObjectReader {
    /// Create a new object reader.
    ///
    /// # Arguments
    /// * `data` — Raw AcDb:AcDbObjects section data
    /// * `dxf_version` — DXF version for version-specific parsing
    /// * `handle_map` — Handle → file offset map from the handle section
    pub fn new(
        data: Vec<u8>,
        dxf_version: DxfVersion,
        handle_map: HashMap<u64, i64>,
    ) -> Result<Self> {
        let version = DwgVersion::from_dxf_version(dxf_version)?;
        Ok(DwgObjectReader {
            data,
            version,
            dxf_version,
            handle_map,
        })
    }

    /// Read a single object record at the given byte offset.
    ///
    /// Returns the type code and a `DwgMergedReader` positioned at the
    /// start of the object's merged data (after framing).
    ///
    /// Record framing:
    /// ```text
    /// [MS(size)][R2010+: MC(handle_bits)][merged_data][CRC16]
    /// ```
    pub fn read_record_at(&self, offset: usize) -> Result<(i16, DwgMergedReader)> {
        if offset >= self.data.len() {
            return Err(DxfError::Parse(format!(
                "Object offset {} out of range (data len {})",
                offset, self.data.len()
            )));
        }

        let mut pos = offset;

        // 1. Read ModularShort (MS) — object size in bytes
        let (size, ms_len) = read_modular_short(&self.data[pos..]);
        pos += ms_len;

        // 2. R2010+: Read ModularChar (MC) — handle stream bit count
        let handle_bits = if self.version.r2010_plus() {
            let (hb, mc_len) = read_modular_char(&self.data[pos..]);
            pos += mc_len;
            hb as i64
        } else {
            0
        };

        // 3. Extract merged data
        if pos + size > self.data.len() {
            return Err(DxfError::Parse(format!(
                "Object record extends past data: offset={}, size={}, data_len={}",
                offset, size, self.data.len()
            )));
        }
        let merged_data = self.data[pos..pos + size].to_vec();

        // 4. For R2007 (ThreeStream, non-R2010): read type_code and
        //    total_size_bits from a temp reader first, then manually
        //    construct the three-stream readers with correct positions.
        //    The ThreeStream constructor in DwgMergedReader::new reads
        //    from position 0 expecting a BL, but position 0 has BS(type_code).
        if self.version.r2007_plus() && !self.version.r2010_plus() {
            let dwg = DwgVersion::from_dxf_version(self.dxf_version)
                .unwrap_or(DwgVersion::AC15);

            // Read type_code + total_size_bits from temp reader
            let mut temp = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
                merged_data.clone(), dwg, self.dxf_version,
            );
            let type_code = temp.read_object_type();
            let _saved_pos = temp.position_in_bits(); // position of the RL
            let total_size_bits = temp.read_raw_long() as i64;
            let data_start_bits = temp.position_in_bits();

            // Per-object RL convention (matches ACadSharp):
            // RL = absolute bit position of handle stream start (from bit 0).
            // Flag bit is at RL - 1.  Handles start at bit RL (NOT byte-aligned).
            let flag_position = total_size_bits - 1;

            // Main reader: starts at data_start_bits (after type_code + RL)
            let mut main_reader = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
                merged_data.clone(), dwg, self.dxf_version,
            );
            main_reader.set_position_in_bits(data_start_bits);

            // Text reader: positioned by flag at flag_position
            let mut text_reader = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
                merged_data.clone(), dwg, self.dxf_version,
            );
            text_reader.set_position_by_flag(flag_position);

            // Handle reader: starts at bit position total_size_bits (NOT byte-aligned).
            // This matches ACadSharp's convention for per-object records.
            let mut handle_reader = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
                merged_data.clone(), dwg, self.dxf_version,
            );
            handle_reader.set_position_in_bits(total_size_bits);

            let reader = DwgMergedReader::from_readers(
                main_reader,
                Some(text_reader),
                Some(handle_reader),
                self.dxf_version,
            );
            return Ok((type_code, reader));
        }

        // 5. Create merged reader (R2010+ or pre-R2007)
        let reader = DwgMergedReader::new(merged_data, self.dxf_version, handle_bits);

        // 6. Read the type code from the reader
        let mut reader = reader;
        let type_code = reader.read_object_type();

        Ok((type_code, reader))
    }

    /// Read common data shared by all objects (entities and non-entities).
    ///
    /// This reads: object_type (already read), size placeholder, handle, xdata.
    /// The type_code is passed in since it was already read during dispatch.
    pub fn read_common_data(
        &self,
        reader: &mut DwgMergedReader,
        type_code: i16,
    ) -> ObjectCommonData {
        // R2000..R2004: deferred size field (BL placeholder — skip it)
        // For R2007+, this was already consumed during record parsing.
        if self.version.r2000_plus() && !self.version.r2007_plus() {
            let _size_in_bits = reader.read_raw_long();
        }

        // Handle (absolute, from main stream)
        let handle = reader.main_mut().read_handle();

        // Extended data (read and skip)
        let xdata_size = self.skip_extended_data(reader);

        ObjectCommonData {
            type_code,
            handle,
            xdata_size,
        }
    }

    /// Read common entity data (the full entity preamble).
    ///
    /// Field order matches the writer exactly.
    pub fn read_common_entity_data(
        &self,
        reader: &mut DwgMergedReader,
        type_code: i16,
    ) -> EntityCommonData {
        let common = self.read_common_data(reader, type_code);

        // Graphic presence flag
        let has_graphic = reader.read_bit();
        if has_graphic {
            // Skip graphic data (preview image)
            let graphic_size = reader.read_raw_long();
            for _ in 0..safe_count(graphic_size as i32) {
                reader.read_byte();
            }
        }

        // R13-R14: save position for size
        if self.version.r13_14_only() {
            let _size = reader.read_raw_long();
        }

        // Entity mode (2 bits)
        let entity_mode = reader.main_mut().read_2bits();

        // Owner handle (if entity_mode == 0)
        let owner_handle = if entity_mode == 0 {
            reader.read_handle()
        } else {
            0
        };

        // Reactor count + handles
        let reactor_count = safe_count(reader.read_bit_long());
        let mut reactors = Vec::new();
        for _ in 0..reactor_count {
            reactors.push(reader.read_handle());
        }

        // XDictionary
        // R2004+: xdic_missing_flag (B) + conditional handle
        // Pre-R2004: always read handle (0 = none)
        let xdictionary_handle = if self.version.r2004_plus() {
            let no_xdic = reader.read_bit();
            if !no_xdic {
                Some(reader.read_handle())
            } else {
                None
            }
        } else {
            let h = reader.read_handle();
            if h != 0 { Some(h) } else { None }
        };

        // R2013+: binary data flag
        if self.version.r2013_plus(self.dxf_version) {
            let _has_binary_data = reader.read_bit();
        }

        // R13-R14: layer + linetype
        let mut layer_handle = 0u64;
        let mut linetype_flags = 0u8;
        if self.version.r13_14_only() {
            layer_handle = reader.read_handle();
            let _is_bylayer_lt = reader.read_bit();
            // If not by-layer, would read linetype handle
        }

        // Pre-R2004: Nolinks + prev/next
        let mut prev_entity_handle = None;
        let mut next_entity_handle = None;
        if !self.version.r2004_plus() && self.version.r2000_plus() {
            let nolinks = reader.read_bit();
            if !nolinks {
                prev_entity_handle = Some(reader.read_handle());
                next_entity_handle = Some(reader.read_handle());
            }
        }

        // Color
        let (color, transparency) = if self.version.r2000_plus() {
            let (c, t, _has_handle) = reader.read_en_color();
            (c, t)
        } else {
            (reader.read_cm_color(), Transparency::default())
        };

        // Linetype scale
        let linetype_scale = reader.read_bit_double();

        // R13-R14: invisibility + return early
        let invisible;
        if self.version.r13_14_only() {
            invisible = reader.read_bit_short() != 0;
            return EntityCommonData {
                common,
                has_graphic,
                entity_mode,
                owner_handle,
                reactors,
                xdictionary_handle,
                color,
                transparency,
                line_weight: 0,
                linetype_scale,
                invisible,
                layer_handle,
                linetype_flags,
                prev_entity_handle,
                next_entity_handle,
            };
        }

        // R2000+: Layer handle
        if self.version.r2000_plus() {
            layer_handle = reader.read_handle();
        }

        // Linetype flags (2 bits)
        linetype_flags = reader.main_mut().read_2bits();

        // R2007+: material flags + shadow flags
        if self.version.r2007_plus() {
            let _material_flags = reader.main_mut().read_2bits();
            let _shadow_flags = reader.read_byte();
        }

        // R2000+: Plotstyle flags
        if self.version.r2000_plus() {
            let _plotstyle_flags = reader.main_mut().read_2bits();
        }

        // R2010+: visual style bits
        if self.version.r2010_plus() {
            let _has_full_visual = reader.read_bit();
            let _has_face_visual = reader.read_bit();
            let _has_edge_visual = reader.read_bit();
        }

        // Invisibility
        invisible = reader.read_bit_short() != 0;

        // R2000+: Lineweight
        let line_weight = if self.version.r2000_plus() {
            reader.read_byte()
        } else {
            0
        };

        EntityCommonData {
            common,
            has_graphic,
            entity_mode,
            owner_handle,
            reactors,
            xdictionary_handle,
            color,
            transparency,
            line_weight,
            linetype_scale,
            invisible,
            layer_handle,
            linetype_flags,
            prev_entity_handle,
            next_entity_handle,
        }
    }

    /// Read common non-entity object data.
    pub fn read_common_non_entity_data(
        &self,
        reader: &mut DwgMergedReader,
        type_code: i16,
    ) -> NonEntityCommonData {
        // Read common data (type + size + handle + xdata)
        let common = self.read_common_data(reader, type_code);

        // R13-R14: size placeholder
        if self.version.r13_14_only() {
            let _size = reader.read_raw_long();
        }

        // Owner handle (soft pointer)
        let owner_handle = reader.read_handle();

        // Reactor count + handles
        let reactor_count = safe_count(reader.read_bit_long());
        let mut reactors = Vec::new();
        for _ in 0..reactor_count {
            reactors.push(reader.read_handle());
        }

        // XDictionary
        // R2004+: xdic_missing_flag (B) + conditional handle
        // Pre-R2004: always read handle (0 = none)
        let xdictionary_handle = if self.version.r2004_plus() {
            let no_xdic = reader.read_bit();
            if !no_xdic {
                Some(reader.read_handle())
            } else {
                None
            }
        } else {
            let h = reader.read_handle();
            if h != 0 { Some(h) } else { None }
        };

        // R2013+: binary data flag
        if self.version.r2013_plus(self.dxf_version) {
            let _has_binary_data = reader.read_bit();
        }

        NonEntityCommonData {
            common,
            owner_handle,
            reactors,
            xdictionary_handle,
        }
    }

    /// Skip extended data and return the total size skipped.
    ///
    /// XData format: BL app_count, for each: TV app_name + items until end marker.
    /// For now, we just read the count and skip by reading zero items
    /// (since the writer writes count=0).
    fn skip_extended_data(&self, reader: &mut DwgMergedReader) -> usize {
        // XData: BS count of registered applications with xdata
        let count = reader.read_bit_short();
        if count <= 0 {
            return 0;
        }

        // TODO: Full xdata parsing — for now we expect count=0
        // from our writer. If non-zero, we'd need to parse each block.
        let mut total_size: usize = 0;
        for _ in 0..count {
            // Each app block: RS(app_size) + data items
            let app_size = reader.read_raw_short();
            let app_size_u = app_size.max(0) as usize;
            total_size = total_size.wrapping_add(app_size_u);
            // Read handle reference for the app
            let _ = reader.read_handle();
            // Read xdata items (app_size bytes worth)
            self.skip_xdata_items(reader, app_size_u);
        }
        total_size
    }

    /// Skip xdata items for a single application block.
    fn skip_xdata_items(&self, _reader: &mut DwgMergedReader, _remaining: usize) {
        // Each xdata item starts with a group code byte:
        // 0=string, 2=open_brace, 3=layer_ref, 5=handle,
        // 10/20/30=3RD, 40=BD, 70=BS, 71=BL
        // For now, this is a stub — our writer never produces xdata.
        // Full implementation would parse by group code until end marker.
    }

    /// Get the list of all handles in the handle map.
    pub fn handles(&self) -> Vec<u64> {
        self.handle_map.keys().copied().collect()
    }

    /// Get the byte offset for a given handle.
    pub fn offset_for(&self, handle: u64) -> Option<i64> {
        self.handle_map.get(&handle).copied()
    }

    /// Get the DWG version.
    pub fn version(&self) -> DwgVersion {
        self.version
    }

    /// Get the DXF version.
    pub fn dxf_version(&self) -> DxfVersion {
        self.dxf_version
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Modular encoding readers (byte-level, for record framing)
// ════════════════════════════════════════════════════════════════════════════

/// Read a ModularShort (MS) from a byte slice.
/// Returns (value, bytes_consumed).
fn read_modular_short(data: &[u8]) -> (usize, usize) {
    let mut value: usize = 0;
    let mut shift = 0;
    let mut i = 0;

    loop {
        if i + 1 >= data.len() {
            break;
        }
        let word = u16::from_le_bytes([data[i], data[i + 1]]);
        i += 2;

        value |= ((word & 0x7FFF) as usize) << shift;
        shift += 15;

        if (word & 0x8000) == 0 {
            break;
        }
    }

    (value, i)
}

/// Read a ModularChar (MC) from a byte slice.
/// Returns (value, bytes_consumed).
fn read_modular_char(data: &[u8]) -> (usize, usize) {
    let mut value: usize = 0;
    let mut shift = 0;
    let mut i = 0;

    loop {
        if i >= data.len() {
            break;
        }
        let b = data[i];
        i += 1;

        value |= ((b & 0x7F) as usize) << shift;
        shift += 7;

        if (b & 0x80) == 0 {
            break;
        }
    }

    (value, i)
}

// ════════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modular_short_read() {
        // Single word: value 42 (0x002A), no continuation
        let data = [0x2A, 0x00];
        let (val, len) = read_modular_short(&data);
        assert_eq!(val, 42);
        assert_eq!(len, 2);
    }

    #[test]
    fn test_modular_short_multi_word() {
        // Two words: 0x8001 0x0001 → (1) | (1 << 15) = 1 + 32768 = 32769
        let data = [0x01, 0x80, 0x01, 0x00];
        let (val, len) = read_modular_short(&data);
        assert_eq!(val, 32769);
        assert_eq!(len, 4);
    }

    #[test]
    fn test_modular_char_read() {
        // Single byte: value 42 (no continuation)
        let data = [42u8];
        let (val, len) = read_modular_char(&data);
        assert_eq!(val, 42);
        assert_eq!(len, 1);
    }

    #[test]
    fn test_modular_char_multi_byte() {
        // Two bytes: 0x81 0x01 → (1) | (1 << 7) = 1 + 128 = 129
        let data = [0x81, 0x01];
        let (val, len) = read_modular_char(&data);
        assert_eq!(val, 129);
        assert_eq!(len, 2);
    }

    #[test]
    fn test_modular_short_roundtrip() {
        // Verify roundtrip with the writer's encoding
        use crate::io::dwg::dwg_stream_writers::object_writer::common::{
            write_modular_short_bytes, write_modular_char_bytes,
        };

        for &val in &[0, 1, 42, 127, 128, 255, 1000, 32767, 32768, 65535, 100000] {
            let mut buf = Vec::new();
            write_modular_short_bytes(&mut buf, val);
            let (read_val, _) = read_modular_short(&buf);
            assert_eq!(read_val, val, "MS roundtrip failed for {}", val);
        }

        for &val in &[0, 1, 42, 127, 128, 255, 1000, 32767, 65535, 100000] {
            let mut buf = Vec::new();
            write_modular_char_bytes(&mut buf, val);
            let (read_val, _) = read_modular_char(&buf);
            assert_eq!(read_val, val, "MC roundtrip failed for {}", val);
        }
    }

    #[test]
    fn test_read_record_roundtrip() {
        // Write a simple object record using the writer, then read it back
        use crate::io::dwg::dwg_stream_writers::object_writer::common::write_modular_short_bytes;
        use crate::io::dwg::dwg_stream_writers::merged_writer::DwgMergedWriter;
        use crate::io::dwg::crc;

        let dwg_ver = DwgVersion::AC15;
        let dxf_ver = DxfVersion::AC1015;

        // Build merged data: type_code(BS) + size_placeholder(BL) + handle
        let mut writer = DwgMergedWriter::new(dwg_ver, dxf_ver);
        writer.write_object_type(common::OBJ_LINE); // type = 19
        writer.save_position_for_size();
        writer.main_mut().write_handle_undefined(0x42);
        // XData: count=0
        writer.write_bit_short(0);

        let merged = writer.merge();

        // Build the record frame: MS(size) + merged + CRC16
        let mut record = Vec::new();
        write_modular_short_bytes(&mut record, merged.len());
        record.extend_from_slice(&merged);
        let crc_val = crc::crc16(crc::CRC16_SEED, &record);
        record.extend_from_slice(&crc_val.to_le_bytes());

        // Create the reader
        let handle_map = HashMap::new();
        let reader = DwgObjectReader::new(record, dxf_ver, handle_map).unwrap();

        // Read the record at offset 0
        let (type_code, mut merged_reader) = reader.read_record_at(0).unwrap();
        assert_eq!(type_code, common::OBJ_LINE);

        // Read common data (size BL + handle + xdata)
        let common_data = reader.read_common_data(&mut merged_reader, type_code);
        assert_eq!(common_data.type_code, common::OBJ_LINE);
        assert_eq!(common_data.handle, 0x42);
        assert_eq!(common_data.xdata_size, 0);
    }
}
