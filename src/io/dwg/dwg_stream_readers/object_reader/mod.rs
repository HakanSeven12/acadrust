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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    /// Linetype handle (only valid if linetype_flags == 0b11)
    pub linetype_handle: u64,
    /// Previous entity handle (pre-R2004)
    pub prev_entity_handle: Option<u64>,
    /// Next entity handle (pre-R2004)
    pub next_entity_handle: Option<u64>,
}

/// Common non-entity object data.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

        // 4. For R2007+ (ThreeStream): read type_code from a temp reader,
        //    then manually construct the three sub-readers with correct
        //    stream positions.
        //
        //    R2007 (AC1021): record contains an RL (total_size_bits) field
        //      after the type code.  Stream positions are derived from RL.
        //
        //    R2010+ (AC1024+): NO embedded RL field.  Stream positions are
        //      computed from handle_bits (MC in the record framing).
        //      handle_start = total_data_bits - handle_bits
        //      flag_position = handle_start - 1
        if self.version.r2007_plus() {
            let dwg = DwgVersion::from_dxf_version(self.dxf_version)
                .unwrap_or(DwgVersion::AC15);

            // Read type_code from temp reader
            let mut temp = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
                merged_data.clone(), dwg, self.dxf_version,
            );
            let type_code = temp.read_object_type();

            // Compute flag_position and handle_start
            let (flag_position, handle_start, data_start_bits);

            if !self.version.r2010_plus() {
                // R2007: RL is embedded after type_code
                let _saved_pos = temp.position_in_bits();
                let total_size_bits = temp.read_raw_long() as i64;
                data_start_bits = temp.position_in_bits();

                // Per-object RL convention (matches ACadSharp):
                // RL = absolute bit position of handle stream start (from bit 0).
                // Flag bit is at RL - 1.  Handles start at bit RL (NOT byte-aligned).
                flag_position = total_size_bits - 1;
                handle_start = total_size_bits;
            } else {
                // R2010+: No RL field.  Compute from handle_bits (MC).
                // handle_bits = total_data_bits - handle_start_bits (writer formula)
                // So: handle_start = total_data_bits - handle_bits
                data_start_bits = temp.position_in_bits();
                let total_data_bits = (size as i64) * 8;
                handle_start = total_data_bits - handle_bits;
                flag_position = handle_start - 1;
            }

            // Main reader: starts at data_start_bits (after type_code [+ RL])
            let mut main_reader = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
                merged_data.clone(), dwg, self.dxf_version,
            );
            main_reader.set_position_in_bits(data_start_bits);

            // Text reader: positioned by flag at flag_position
            let mut text_reader = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
                merged_data.clone(), dwg, self.dxf_version,
            );
            text_reader.set_position_by_flag(flag_position);

            // Handle reader: starts at bit position handle_start (NOT byte-aligned).
            let mut handle_reader = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
                merged_data.clone(), dwg, self.dxf_version,
            );
            handle_reader.set_position_in_bits(handle_start);

            let mut reader = DwgMergedReader::from_readers(
                main_reader,
                Some(text_reader),
                Some(handle_reader),
                self.dxf_version,
            );
            reader.set_handle_bits(handle_bits);
            return Ok((type_code, reader));
        }

        // 5. Create merged reader (pre-R2007, TwoStream)
        //
        //    For R2000–R2004 the merged data layout is:
        //        [BS type_code][RL main_size_bits][H handle][xdata]
        //        [...entity/object data...][handle references]
        //    The RL value (main_size_bits) tells us the bit position
        //    where the handle references begin in the merged data.
        //    The writer writes handle bytes at this exact bit position
        //    (which may NOT be byte-aligned).
        //
        //    For R13–R14 there is no top-level RL; we pass 0 and
        //    handle reads fall back to position 0.
        let dwg = DwgVersion::from_dxf_version(self.dxf_version)
            .unwrap_or(DwgVersion::AC15);

        let handle_start_bits = if self.version.r2000_plus() {
            // Read BS + RL from a disposable temp reader to discover
            // the split point without consuming from the final reader.
            let mut temp = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
                merged_data.clone(), dwg, self.dxf_version,
            );
            let _tc = temp.read_object_type(); // BS
            temp.read_raw_long() as i64 // RL = main_size_bits
        } else {
            // R13–R14: no top-level RL.
            0
        };

        // Create main reader (reads from bit 0)
        let main_reader = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
            merged_data.clone(), dwg, self.dxf_version,
        );

        // Create handle reader positioned at handle_start_bits
        let mut handle_reader = crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader::new(
            merged_data, dwg, self.dxf_version,
        );
        handle_reader.set_position_in_bits(handle_start_bits);

        let mut reader = DwgMergedReader::from_readers(
            main_reader,
            None, // no text stream for pre-R2007
            Some(handle_reader),
            self.dxf_version,
        );

        // 6. Read the type code from the reader
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

        // Set this object's handle as the reference for subsequent
        // offset-based handle codes (6/8/A/C) in the handle stream.
        reader.set_ref_handle(handle);

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
            // R2010+: BLL (Bit Long Long); pre-R2010: RL (Raw Long)
            let graphic_size = if self.version.r2010_plus() {
                reader.read_bit_long_long()
            } else {
                reader.read_raw_long() as i64
            };
            for _ in 0..graphic_size.max(0) {
                reader.read_byte();
            }
        }

        // R13-R14: size field (RL = main_size_bits) — reposition handle reader
        if self.version.r13_14_only() {
            let main_size_bits = reader.read_raw_long();
            reader.reposition_handle_reader(main_size_bits);
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
            let is_bylayer_lt = reader.read_bit();
            if !is_bylayer_lt {
                // Linetype handle (hard pointer) — present if NOT by-layer
                let _linetype_handle = reader.read_handle();
            }
        }

        // Pre-R2004: Nolinks + prev/next (R13/R14 and R2000-R2002)
        let mut prev_entity_handle = None;
        let mut next_entity_handle = None;
        if !self.version.r2004_plus() {
            let nolinks = reader.read_bit();
            if !nolinks {
                prev_entity_handle = Some(reader.read_handle());
                next_entity_handle = Some(reader.read_handle());
            }
        }

        // Color
        let (color, transparency, has_color_handle) = if self.version.r2000_plus() {
            reader.read_en_color()
        } else {
            (reader.read_cm_color(), Transparency::default(), false)
        };

        // R2004+: Color book color handle (hard pointer) — only if flagged
        if self.version.r2004_plus() && has_color_handle {
            let _color_book_handle = reader.read_handle();
        }

        // Linetype scale
        let linetype_scale = reader.read_bit_double();

        // R13-R14: invisibility + return early
        // DXF group 60 convention (all DWG versions): 0 = visible, non-zero = invisible
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
                linetype_handle: 0,
                prev_entity_handle,
                next_entity_handle,
            };
        }

        // R2000+: Layer handle
        if self.version.r2000_plus() {
            layer_handle = reader.read_handle();
        }

        // Linetype flags (2 bits): 00=bylayer, 01=byblock, 10=continuous, 11=handle present
        let mut linetype_handle = 0u64;
        linetype_flags = reader.main_mut().read_2bits();
        if linetype_flags == 0b11 {
            // Linetype handle (hard pointer) — present when flags == 11
            linetype_handle = reader.read_handle();
        }

        // R2007+: material flags + shadow flags
        if self.version.r2007_plus() {
            let material_flags = reader.main_mut().read_2bits();
            // Material handle (hard pointer) — present when flags == 0b11
            if material_flags == 0b11 {
                let _material_handle = reader.read_handle();
            }
            let _shadow_flags = reader.read_byte();
        }

        // R2000+: Plotstyle flags (00=bylayer, 01=byblock, 11=handle present)
        if self.version.r2000_plus() {
            let plotstyle_flags = reader.main_mut().read_2bits();
            if plotstyle_flags == 0b11 {
                // Plotstyle handle (hard pointer)
                let _plotstyle_handle = reader.read_handle();
            }
        }

        // R2010+: visual style bits — each bit conditionally followed by a handle
        if self.version.r2010_plus() {
            if reader.read_bit() {
                let _full_visual_style_handle = reader.read_handle();
            }
            if reader.read_bit() {
                let _face_visual_style_handle = reader.read_handle();
            }
            if reader.read_bit() {
                let _edge_visual_style_handle = reader.read_handle();
            }
        }

        // Invisibility
        invisible = reader.read_bit_short() != 0;

        // R2000+: Lineweight (5-bit DWG index → raw byte value)
        let line_weight = if self.version.r2000_plus() {
            let idx = reader.read_byte();
            crate::types::LineWeight::from_dwg_index(idx).as_i16() as u8
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
            linetype_handle,
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

        // R13-R14: size field (RL = main_size_bits) — reposition handle reader
        if self.version.r13_14_only() {
            let main_size_bits = reader.read_raw_long();
            reader.reposition_handle_reader(main_size_bits);
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
    /// EED format: repeating [BS size | H app_handle | RC×size data] until size==0.
    /// The application handle is ALWAYS in the main stream (even for R2007+),
    /// unlike entity reference handles which go to the handle stream.
    fn skip_extended_data(&self, reader: &mut DwgMergedReader) -> usize {
        // EED: repeating [ BS size (main) | H app_handle (main!) | RC×size data (main) ] until size==0
        let mut total_size: usize = 0;
        loop {
            let size = reader.read_bit_short();
            if size <= 0 {
                break;
            }
            let size_u = size as usize;
            total_size += size_u;
            // Application handle — always from MAIN stream (not handle stream).
            // Per ACadSharp reference: EED handles are inline in the object data,
            // not in the handle chain.
            let _ = reader.main_mut().read_handle();
            // Raw xdata bytes (from main stream)
            self.skip_xdata_items(reader, size_u);
        }
        total_size
    }

    /// Skip xdata items for a single application block.
    fn skip_xdata_items(&self, reader: &mut DwgMergedReader, remaining: usize) {
        // Each EED application block has `remaining` raw bytes of xdata
        // items in the main stream.  We must consume them to keep the
        // bit position aligned for subsequent reads.
        for _ in 0..remaining {
            reader.read_byte();
        }
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
