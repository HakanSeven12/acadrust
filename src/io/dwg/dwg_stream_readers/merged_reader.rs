//! DWG Merged Reader — R2007+ three-stream demultiplexer
//!
//! In DWG R2007+, each object/section record is encoded as three
//! interleaved streams:
//!
//! ```text
//! |---main---|---text---|flag|---handles---|
//! ```
//!
//! `DwgMergedReader` transparently routes reads to the correct sub-reader:
//! - Data reads → main reader
//! - `read_variable_text()` → text reader  
//! - `read_handle()` → handle reader
//!
//! For pre-R2007, all reads go to the main reader (two-stream mode where
//! text is inline in the main stream).
//!
//! Based on ACadSharp's `DwgMergedReader`.

use crate::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
use crate::io::dwg::dwg_version::DwgVersion;
use crate::types::{Color, DxfVersion, Vector2, Vector3};

/// Merge mode, determined by DWG version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MergeMode {
    /// R13–R2004: Two-stream (main + handle). Text is inline in main.
    TwoStream,
    /// R2007+: Three-stream (main + text + handle).
    ThreeStream,
}

/// Merged reader that transparently demultiplexes three-stream R2007+ data.
///
/// For pre-R2007, this acts as a simple passthrough to the main reader.
pub struct DwgMergedReader {
    /// Main data reader
    main: DwgBitReader,
    /// Text reader (R2007+ only; for pre-R2007, text reads from main)
    text: Option<DwgBitReader>,
    /// Handle reader (split from main after handle_start_bits)
    handle: Option<DwgBitReader>,
    /// Merge mode
    _mode: MergeMode,
    /// DXF version
    dxf_version: DxfVersion,
    /// Raw data (kept for lazy text/handle setup in ThreeStream mode)
    raw_data: Option<Vec<u8>>,
}

impl DwgMergedReader {
    /// Create a merged reader from raw section data.
    ///
    /// For R2007+, this splits the data into three sub-readers based on
    /// the embedded stream boundaries.
    ///
    /// For pre-R2007, the data is split into main (up to handle_start_bits)
    /// and handle (from handle_start_bits onward).
    ///
    /// # Arguments
    /// * `data` - Raw section data (the merged stream bytes)
    /// * `dxf_version` - DXF version for version-specific parsing
    /// * `handle_start_bits` - Bit position where handle data begins
    ///   (only used for two-stream mode; for three-stream, computed from flags)
    pub fn new(
        data: Vec<u8>,
        dxf_version: DxfVersion,
        handle_start_bits: i64,
    ) -> Self {
        let dwg = DwgVersion::from_dxf_version(dxf_version)
            .unwrap_or(DwgVersion::AC15);

        let mode = if dxf_version >= DxfVersion::AC1021 {
            MergeMode::ThreeStream
        } else {
            MergeMode::TwoStream
        };

        match mode {
            MergeMode::TwoStream => {
                // Two-stream: main = data[:handle_start], handle = data[handle_start:]
                let main = DwgBitReader::new(data.clone(), dwg, dxf_version);

                // Create handle reader from remaining bytes
                let handle_start_byte = (handle_start_bits / 8) as usize;
                let handle_data = if handle_start_byte < data.len() {
                    data[handle_start_byte..].to_vec()
                } else {
                    Vec::new()
                };
                let handle = DwgBitReader::new(handle_data, dwg, dxf_version);

                DwgMergedReader {
                    main,
                    text: None,
                    handle: Some(handle),
                    _mode: mode,
                    dxf_version,
                    raw_data: None,
                }
            }
            MergeMode::ThreeStream => {
                // Three-stream: lazy setup.
                // Don't read BL or set up text/handle readers here.
                // The BL is not at position 0 — it comes after the type code.
                // Text and handle readers will be set up later via
                // setup_text_and_handle() after the caller reads the BL.
                let main_reader = DwgBitReader::new(data.clone(), dwg, dxf_version);

                DwgMergedReader {
                    main: main_reader,
                    text: None,
                    handle: None,
                    _mode: mode,
                    dxf_version,
                    raw_data: Some(data),
                }
            }
        }
    }

    /// Create a merged reader from separate pre-split streams.
    ///
    /// Used when the caller has already separated the three streams
    /// (e.g., after decompression of individual section pages).
    pub fn from_readers(
        main: DwgBitReader,
        text: Option<DwgBitReader>,
        handle: Option<DwgBitReader>,
        dxf_version: DxfVersion,
    ) -> Self {
        let mode = if text.is_some() {
            MergeMode::ThreeStream
        } else {
            MergeMode::TwoStream
        };
        DwgMergedReader {
            main,
            text,
            handle,
            _mode: mode,
            dxf_version,
            raw_data: None,
        }
    }

    /// Set up text and handle readers for ThreeStream mode.
    ///
    /// Called after the caller reads the BL (total_size_bits) from the main stream.
    /// The BL is written by `save_position_for_size` in the writer, after the type code.
    /// It stores the bit position of the text-present flag relative to position 0.
    ///
    /// Layout: `[type_code][BL][main_data...][text...][modular_short][flag@BL][handles...]`
    pub fn setup_text_and_handle(&mut self, total_size_bits: i64) {
        if let Some(ref data) = self.raw_data {
            let dwg = DwgVersion::from_dxf_version(self.dxf_version)
                .unwrap_or(DwgVersion::AC15);

            // Text reader — positioned via the flag mechanism at total_size_bits.
            // set_position_by_flag reads the text-present flag at total_size_bits.
            // If the flag is FALSE, the reader is marked as empty — but we still
            // store it so that read_variable_text() returns empty strings rather
            // than consuming text-sized data from the main stream. This preserves
            // alignment even when the text encoding convention differs between
            // the writing application and our reader.
            let mut text_reader = DwgBitReader::new(data.clone(), dwg, self.dxf_version);
            text_reader.set_position_by_flag(total_size_bits);
            self.text = Some(text_reader);

            // Handle reader — starts at the next byte boundary after the flag bit.
            // The writer pads to byte boundary after the flag (write_spear_shift),
            // matching the object reader formula: ((end_bit + 1 + 7) / 8) * 8.
            let handle_start = ((total_size_bits + 1 + 7) / 8) * 8;
            let mut handle_reader = DwgBitReader::new(data.clone(), dwg, self.dxf_version);
            handle_reader.set_position_in_bits(handle_start);
            self.handle = Some(handle_reader);
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    //  Data reads — always from main reader
    // ════════════════════════════════════════════════════════════════════════

    pub fn read_bit(&mut self) -> bool { self.main.read_bit() }
    pub fn read_byte(&mut self) -> u8 { self.main.read_byte() }
    pub fn read_bit_short(&mut self) -> i16 { self.main.read_bit_short() }
    pub fn read_bit_long(&mut self) -> i32 { self.main.read_bit_long() }
    pub fn read_bit_long_long(&mut self) -> i64 { self.main.read_bit_long_long() }
    pub fn read_bit_double(&mut self) -> f64 { self.main.read_bit_double() }
    pub fn read_raw_long(&mut self) -> i64 { self.main.read_raw_long() }
    pub fn read_raw_short(&mut self) -> i16 { self.main.read_raw_short() }
    pub fn read_raw_double(&mut self) -> f64 { self.main.read_raw_double() }
    pub fn read_2bit_double(&mut self) -> Vector2 { self.main.read_2bit_double() }
    pub fn read_3bit_double(&mut self) -> Vector3 { self.main.read_3bit_double() }
    pub fn read_2raw_double(&mut self) -> Vector2 { self.main.read_2raw_double() }
    pub fn read_bit_extrusion(&mut self) -> Vector3 { self.main.read_bit_extrusion() }
    pub fn read_bit_thickness(&mut self) -> f64 { self.main.read_bit_thickness() }
    pub fn read_bit_double_with_default(&mut self, default: f64) -> f64 {
        self.main.read_bit_double_with_default(default)
    }
    pub fn read_cm_color(&mut self) -> Color { self.main.read_cm_color() }
    pub fn read_en_color(&mut self) -> (Color, crate::types::Transparency, bool) {
        self.main.read_en_color()
    }
    pub fn read_color_by_index(&mut self) -> Color { self.main.read_color_by_index() }
    pub fn read_modular_char(&mut self) -> u64 { self.main.read_modular_char() }
    pub fn read_signed_modular_char(&mut self) -> i64 { self.main.read_signed_modular_char() }
    pub fn read_modular_short(&mut self) -> i32 { self.main.read_modular_short() }
    pub fn read_object_type(&mut self) -> i16 { self.main.read_object_type() }

    // ════════════════════════════════════════════════════════════════════════
    //  Text reads — from text reader for R2007+, main for pre-R2007
    // ════════════════════════════════════════════════════════════════════════

    /// Read a variable-length text string.
    ///
    /// For R2007+, this reads from the separate text stream (UTF-16LE).
    /// For pre-R2007, this reads from the main stream.
    pub fn read_variable_text(&mut self) -> String {
        match &mut self.text {
            Some(text_reader) => text_reader.read_variable_text(),
            None => self.main.read_variable_text(),
        }
    }

    /// Read a text string, but always from the main stream.
    ///
    /// Used for fields that are always inline even in R2007+.
    pub fn read_text_inline(&mut self) -> String {
        self.main.read_variable_text()
    }

    // ════════════════════════════════════════════════════════════════════════
    //  Handle reads — from handle reader if available, else main
    // ════════════════════════════════════════════════════════════════════════

    /// Read a handle reference.
    ///
    /// For R2007+, this reads from the separate handle stream.
    /// For pre-R2007, this reads from the main stream.
    pub fn read_handle(&mut self) -> u64 {
        match &mut self.handle {
            Some(handle_reader) => handle_reader.read_handle(),
            None => self.main.read_handle(),
        }
    }

    /// Read a handle reference relative to a base handle.
    pub fn read_handle_reference(&mut self, ref_handle: u64) -> (u64, crate::io::dwg::dwg_reference_type::DwgReferenceType) {
        match &mut self.handle {
            Some(handle_reader) => {
                let mut ref_type = crate::io::dwg::dwg_reference_type::DwgReferenceType::Undefined;
                let h = handle_reader.read_handle_reference(ref_handle, &mut ref_type);
                (h, ref_type)
            }
            None => {
                let mut ref_type = crate::io::dwg::dwg_reference_type::DwgReferenceType::Undefined;
                let h = self.main.read_handle_reference(ref_handle, &mut ref_type);
                (h, ref_type)
            }
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    //  Position and state queries
    // ════════════════════════════════════════════════════════════════════════

    /// Main reader bit position.
    pub fn position_in_bits(&self) -> i64 { self.main.position_in_bits() }

    /// Main reader byte position.
    pub fn position(&self) -> usize { self.main.position() }

    /// Set main reader position.
    pub fn set_position_in_bits(&mut self, pos: i64) { self.main.set_position_in_bits(pos); }

    /// Get the DXF version.
    pub fn dxf_version(&self) -> DxfVersion { self.dxf_version }

    /// Get a mutable reference to the main reader (for direct access).
    pub fn main_mut(&mut self) -> &mut DwgBitReader { &mut self.main }

    /// Get a reference to the main reader.
    pub fn main(&self) -> &DwgBitReader { &self.main }
}

// ════════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::dwg::dwg_stream_writers::bit_writer::DwgBitWriter;

    #[test]
    fn test_two_stream_from_readers() {
        // Create main stream with data + text
        let dwg = DwgVersion::AC15;
        let version = DxfVersion::AC1015;
        let mut main_writer = DwgBitWriter::new(dwg, version);
        main_writer.write_bit_short(42);
        main_writer.write_bit_double(3.14);
        main_writer.write_variable_text("hello");

        // Create handle stream
        let mut handle_writer = DwgBitWriter::new(dwg, version);
        handle_writer.write_handle_undefined(0x1A);

        let main_data = main_writer.to_bytes();
        let handle_data = handle_writer.to_bytes();

        let main = DwgBitReader::new(main_data, dwg, version);
        let handle = DwgBitReader::new(handle_data, dwg, version);

        let mut reader = DwgMergedReader::from_readers(main, None, Some(handle), version);

        assert_eq!(reader.read_bit_short(), 42);
        assert!((reader.read_bit_double() - 3.14).abs() < 1e-10);
        assert_eq!(reader.read_variable_text(), "hello");

        let h = reader.read_handle();
        assert_eq!(h, 0x1A);
    }

    #[test]
    fn test_from_readers_passthrough() {
        let dwg = DwgVersion::AC15;
        let version = DxfVersion::AC1015;

        let mut writer = DwgBitWriter::new(dwg, version);
        writer.write_bit_short(99);
        writer.write_bit_double(2.71);
        let data = writer.to_bytes();

        let main = DwgBitReader::new(data, dwg, version);
        let mut reader = DwgMergedReader::from_readers(main, None, None, version);

        assert_eq!(reader.read_bit_short(), 99);
        assert!((reader.read_bit_double() - 2.71).abs() < 1e-10);
    }

    #[test]
    fn test_three_stream_text_routing() {
        // Verify that text reads go to the text reader when one is provided
        let dwg = DwgVersion::AC21;
        let version = DxfVersion::AC1021;

        // Main stream: numeric data
        let mut main_writer = DwgBitWriter::new(dwg, version);
        main_writer.write_bit_short(77);
        main_writer.write_bit_double(1.5);
        let main_data = main_writer.to_bytes();

        // Text stream: also numeric data, but routed separately
        // We write text using the TU format (write_text_unicode)
        let mut text_writer = DwgBitWriter::new(dwg, version);
        text_writer.write_variable_text("world");
        let text_data = text_writer.to_bytes();

        // Handle stream
        let mut handle_writer = DwgBitWriter::new(dwg, version);
        handle_writer.write_handle_undefined(0x42);
        let handle_data = handle_writer.to_bytes();

        let main = DwgBitReader::new(main_data, dwg, version);
        let text = DwgBitReader::new(text_data, dwg, version);
        let handle = DwgBitReader::new(handle_data, dwg, version);

        let mut reader = DwgMergedReader::from_readers(main, Some(text), Some(handle), version);

        // Data from main
        assert_eq!(reader.read_bit_short(), 77);
        assert!((reader.read_bit_double() - 1.5).abs() < 1e-10);

        // Text from separate text reader (uses read_text_unicode internally for R2007+)
        let t = reader.read_variable_text();
        assert_eq!(t, "world");

        // Handle from separate handle reader
        let h = reader.read_handle();
        assert_eq!(h, 0x42);
    }
}

