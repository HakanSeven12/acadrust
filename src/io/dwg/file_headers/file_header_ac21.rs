//! AC1021 (R2007) file header writer
//!
//! Implements the page-based DWG file format for R2007, which differs from
//! AC18 in three critical ways:
//!
//! 1. **Reed-Solomon encoding** — all pages are RS-encoded for error correction
//! 2. **LZ77 AC21 compression** — a different LZ77 variant than AC18
//! 3. **CRC-64 checksums** — 64-bit CRC (normal + mirrored) instead of CRC-32
//! 4. **UTF-16LE section names** — section map uses 2-byte Unicode names
//!
//! ## File Layout (spec §5.1)
//!
//! ```text
//! [0x80 bytes:  Metadata (version string, addresses)]
//! [0x400 bytes: File header (RS-encoded compressed metadata)]
//! [0x400 bytes: Page map (system page)]
//! [0x400 bytes: Page map copy]
//! [Data section pages ...]
//! [System page:  Section map]
//! [System page:  Section map copy]
//! [0x400 bytes: File header copy]
//! ```
//!
//! Based on ODA spec sections 5.1–5.13.

use std::io::{Write, Seek, SeekFrom, Cursor};
use byteorder::{LittleEndian, WriteBytesExt};

use crate::error::DxfError;
use crate::types::DxfVersion;
use super::section_definition::{names, ac21_section_info};
use crate::io::dwg::compressor_ac21::compress_ac21;
use crate::io::dwg::crc::{
    dwg_ac21_normal_crc64,
    dwg_ac21_normal_crc64_seed1,
    dwg_ac21_mirrored_crc64,
    dwg_ac21_header_crc64,
    dwg_ac21_page_checksum,
};
use crate::io::dwg::dwg21_metadata::{Dwg21CompressedMetadata, METADATA_SIZE};
use crate::io::dwg::reed_solomon::{
    reed_solomon_encode,
    RS_N, RS_SYSTEM_K, RS_SYSTEM_PRIM_POLY,
    RS_DATA_K, RS_DATA_PRIM_POLY,
};

// ════════════════════════════════════════════════════════════════════════════
//  Constants
// ════════════════════════════════════════════════════════════════════════════

/// Size of the metadata block at the start of the file.
const METADATA_BLOCK_SIZE: usize = 0x80;

/// Size of the RS-encoded file header page.
const FILE_HEADER_PAGE_SIZE: usize = 0x400;

/// Total reserved size at the start: metadata + file header.
const RESERVED_HEADER_SIZE: usize = METADATA_BLOCK_SIZE + FILE_HEADER_PAGE_SIZE;

/// CRC block size (alignment unit) per spec §5.3.1.
const CRC_BLOCK_SIZE: usize = 8;

/// Page alignment size per spec §5.3.1.
const PAGE_ALIGN_SIZE: usize = 0x20;

/// Minimum system page size per spec §5.3.1.
const MIN_SYSTEM_PAGE_SIZE: usize = 0x400;

/// Space reserved for two page map system pages right after the file header.
const PAGE_MAP_RESERVED_SIZE: usize = 2 * MIN_SYSTEM_PAGE_SIZE; // 0x800

/// Total reserved size at the start: metadata + file header + page map pages.
const TOTAL_RESERVED_SIZE: usize = RESERVED_HEADER_SIZE + PAGE_MAP_RESERVED_SIZE; // 0xC80

/// Check data size at end of file header page (5 × u64 = 40 = 0x28 bytes).
const CHECK_DATA_SIZE: usize = 0x28;

/// RS data available in the file header (0x400 - check_data = 0x3D8).
const RS_DATA_IN_HEADER: usize = FILE_HEADER_PAGE_SIZE - CHECK_DATA_SIZE;

/// Factor for file header RS encoding (3 sub-streams).
const FILE_HEADER_RS_FACTOR: usize = 3;

// ════════════════════════════════════════════════════════════════════════════
//  CRC Random Encoder (spec §5.11)
// ════════════════════════════════════════════════════════════════════════════

/// Pseudo-random number generator for CRC encoding (spec §5.11).
///
/// Generates pseudo-random values from a seed for:
/// - CRC seed encoding
/// - Check data generation
/// - Random padding
struct CrcRandomEncoder {
    table: Vec<u32>,
    index: usize,
}

impl CrcRandomEncoder {
    /// Create a new encoder from a 64-bit seed.
    ///
    /// Populates the 0x270 (624) entry table using the Mersenne Twister
    /// initialization algorithm.
    fn new(seed: u64) -> Self {
        let mut table = vec![0u32; 0x270];

        // Initialize first two entries from seed halves
        table[0] = seed as u32;
        table[1] = (seed >> 32) as u32;

        // Mersenne Twister-style initialization
        for i in 2..0x270 {
            table[i] = (table[i - 1] >> 30 ^ table[i - 1])
                .wrapping_mul(0x6C078965)
                .wrapping_add(i as u32);
        }

        let mut encoder = Self { table, index: 0x270 };
        // Generate initial state
        encoder.regenerate();
        encoder
    }

    /// Regenerate the table (Mersenne Twister twist step).
    fn regenerate(&mut self) {
        for i in 0..0x270 {
            let y = (self.table[i] & 0x80000000)
                | (self.table[(i + 1) % 0x270] & 0x7FFFFFFF);
            self.table[i] = self.table[(i + 0x18D) % 0x270] ^ (y >> 1);
            if y & 1 != 0 {
                self.table[i] ^= 0x9908B0DF;
            }
        }
        self.index = 0;
    }

    /// Get the next raw u32 value from the generator.
    fn next_u32(&mut self) -> u32 {
        if self.index >= 0x270 {
            self.regenerate();
        }
        let mut y = self.table[self.index];
        self.index += 1;

        // Tempering
        y ^= y >> 11;
        y ^= (y << 7) & 0x9D2C5680;
        y ^= (y << 15) & 0xEFC60000;
        y ^= y >> 18;
        y
    }

    /// Get the next u64 value (two consecutive u32s).
    fn next_u64(&mut self) -> u64 {
        let lo = self.next_u32() as u64;
        let hi = self.next_u32() as u64;
        lo | (hi << 32)
    }

    /// Encode a CRC seed value using 10-bit XOR obfuscation (spec §5.11).
    ///
    /// Splits the 64-bit seed into 6 groups of 10 bits plus a final 4-bit group.
    /// Each group is XORed with the low bits of a random u32 from the RNG,
    /// then placed at non-overlapping bit positions in the result.
    ///
    /// The spec says "adding bits from a pseudo random encoding table" —
    /// in this context "adding" means XOR, which is its own inverse,
    /// allowing decode by re-running with the same RNG state.
    fn encode_crc_seed(&mut self, seed: u64) -> u64 {
        let mut result: u64 = 0;
        let mut remaining = seed;

        for shift in (0..60).step_by(10) {
            let random = self.next_u32();
            let seed_bits = remaining & 0x3FF;
            let random_bits = (random as u64) & 0x3FF;
            remaining >>= 10;
            result |= (seed_bits ^ random_bits) << shift;
        }
        // Remaining 4 bits at position 60
        let random = self.next_u32();
        let seed_bits = remaining & 0xF;
        let random_bits = (random as u64) & 0xF;
        result |= (seed_bits ^ random_bits) << 60;

        result
    }

    /// Fill a buffer with random bytes.
    fn fill_random(&mut self, buffer: &mut [u8]) {
        let mut i = 0;
        while i + 4 <= buffer.len() {
            let val = self.next_u32();
            buffer[i..i + 4].copy_from_slice(&val.to_le_bytes());
            i += 4;
        }
        if i < buffer.len() {
            let val = self.next_u32();
            let bytes = val.to_le_bytes();
            for j in 0..(buffer.len() - i) {
                buffer[i + j] = bytes[j];
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Section and Page tracking
// ════════════════════════════════════════════════════════════════════════════

/// Record for a single data page written to the stream.
#[derive(Debug, Clone)]
struct AC21PageRecord {
    /// Unique page ID (positive for data pages, negative for gaps).
    id: i64,
    /// Size of the RS-encoded page data in the file stream.
    size: i64,
    /// Absolute file offset where this page starts.
    offset: u64,
}

/// Information about a section and its pages.
#[derive(Debug, Clone)]
struct AC21SectionInfo {
    /// Section name (e.g., "AcDb:Header").
    name: String,
    /// Hash code from spec §5.2.
    hash_code: u32,
    /// Encoding mode (4=compressed, 1=uncompressed).
    encoding: u64,
    /// Default encryption mode.
    encryption: u64,
    /// Max decompressed page size (from spec §5.2 section table).
    max_page_size: u64,
    /// Total uncompressed data size.
    data_size: u64,
    /// Per-page records within this section.
    pages: Vec<AC21SectionPageRecord>,
}

/// Per-page record within a section (for section map serialization).
#[derive(Debug, Clone)]
struct AC21SectionPageRecord {
    /// Offset of this page's data within the decompressed section.
    data_offset: u64,
    /// Size of the RS-encoded page in the file stream.
    page_size: u64,
    /// Page ID (references the page map).
    page_id: i64,
    /// Decompressed size of this page.
    uncompressed_size: u64,
    /// Compressed size (before RS encoding).
    compressed_size: u64,
    /// Adler-32 variant checksum of decompressed data.
    checksum: u64,
    /// CRC-64 of the page.
    crc: u64,
}

// ════════════════════════════════════════════════════════════════════════════
//  Page alignment and system page size calculation (spec §5.3.1)
// ════════════════════════════════════════════════════════════════════════════

/// Align a size to 32-byte (`PAGE_ALIGN_SIZE`) boundary.
fn align32(size: usize) -> usize {
    (size + PAGE_ALIGN_SIZE - 1) & !(PAGE_ALIGN_SIZE - 1)
}

/// Align a page size to `PAGE_ALIGN_SIZE` (0x20) boundary (u64 variant).
#[allow(dead_code)]
fn get_aligned_page_size(page_size: u64) -> u64 {
    (page_size + PAGE_ALIGN_SIZE as u64 - 1) & !(PAGE_ALIGN_SIZE as u64 - 1)
}

/// Calculate the system page size from uncompressed data size (spec §5.3.1).
///
/// The encoded data must fit at least twice in the page, with RS overhead.
#[allow(dead_code)]
fn get_system_page_size(data_size: u64) -> u64 {
    // Align to CRC block size
    let aligned = (data_size + CRC_BLOCK_SIZE as u64 - 1)
        & !(CRC_BLOCK_SIZE as u64 - 1);

    // The page should fit the data at least 2 times (with RS overhead)
    let file_page_size = ((aligned * 2) + RS_SYSTEM_K as u64 - 1)
        / RS_SYSTEM_K as u64
        * RS_N as u64;

    if file_page_size < MIN_SYSTEM_PAGE_SIZE as u64 {
        MIN_SYSTEM_PAGE_SIZE as u64
    } else {
        get_aligned_page_size(file_page_size)
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Encode function (spec §5.2.1.1)
// ════════════════════════════════════════════════════════════════════════════

/// Rotate-left encode used for check data CRC buffers (spec §5.2.1.1).
fn encode_value(value: u64, control: u64) -> u64 {
    let shift = (control & 0x1F) as u32;
    if shift != 0 {
        (value << shift) | (value >> (64 - shift))
    } else {
        value
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  RS encoding helpers for data pages (non-interleaved)
// ════════════════════════════════════════════════════════════════════════════

/// RS-encode data page content with interleaving, matching the reader's decode.
///
/// The RS parameters depend on section encoding type:
/// - encoding=1 → RS(255, 251) with RS_DATA_PRIM_POLY
/// - encoding=4 → RS(255, 239) with RS_SYSTEM_PRIM_POLY
///
/// The factor is computed to match the reader's `get_page_buffer_at`:
/// ```text
/// total_size = (data.len() + 7) & ~7   // CRC block alignment
/// factor = ceil(total_size / block_size)
/// ```
///
/// Returns the RS-encoded interleaved data (factor × 255 bytes).
fn rs_encode_data_page_interleaved(data: &[u8], _encoding: u64) -> Vec<u8> {
    // All data section pages use RS(255, 251) per spec §5.4 / §5.13.
    // System pages (page map, section map) use RS(255, 239) per §5.3,
    // but those are encoded separately in write_system_page().
    // The encoding field (1=stored, 4=compressed) determines only whether
    // LZ77 compression is applied, NOT the RS block size.
    let block_size = RS_DATA_K;      // 251
    let prim_poly = RS_DATA_PRIM_POLY;

    // Match reader's factor computation:
    // total_size = (compressed_size + 7) & ~7   (CRC block alignment)
    // factor = ceil(total_size / block_size)
    let total_size = (data.len() + CRC_BLOCK_SIZE - 1) & !(CRC_BLOCK_SIZE - 1);
    let factor = (total_size + block_size - 1) / block_size;

    // Pad data to factor × block_size bytes
    let padded_len = factor * block_size;
    let mut padded_data = vec![0u8; padded_len];
    let copy_len = data.len().min(padded_len);
    padded_data[..copy_len].copy_from_slice(&data[..copy_len]);

    // RS-encode with interleaving
    let mut encoded = vec![0u8; factor * RS_N];
    reed_solomon_encode(
        &padded_data,
        &mut encoded,
        factor,
        block_size,
        prim_poly,
    );

    encoded
}

// ════════════════════════════════════════════════════════════════════════════
//  Main Writer
// ════════════════════════════════════════════════════════════════════════════

/// AC1021 (R2007) file header writer.
///
/// Follows the same pattern as `DwgFileHeaderWriterAC18`:
/// 1. `new()` reserves header space
/// 2. `add_section()` writes data pages incrementally
/// 3. `write_file()` finalizes with section map, page map, file header, metadata
pub struct DwgFileHeaderWriterAC21 {
    /// DXF version being written.
    version: DxfVersion,
    /// Section info records, keyed by section name.
    sections: Vec<AC21SectionInfo>,
    /// Page map: all pages written so far.
    page_records: Vec<AC21PageRecord>,
    /// CRC-64 seed value.
    crc_seed: u64,
    /// Next page ID to assign.
    next_page_id: i64,
    /// Skip LZ77 compression for debugging (store raw + RS only).
    pub skip_lz77: bool,
}

impl DwgFileHeaderWriterAC21 {
    /// Create a new AC21 file header writer and reserve space.
    ///
    /// Reserves 0xC80 bytes at the start:
    /// - 0x80 bytes: metadata placeholder
    /// - 0x400 bytes: file header placeholder
    /// - 0x800 bytes: page map pages (2 × 0x400)
    pub fn new<W: Write + Seek>(version: DxfVersion, output: &mut W) -> Result<Self, DxfError> {
        // Reserve space for metadata + file header + page map pages
        let zeroes = vec![0u8; TOTAL_RESERVED_SIZE];
        output.write_all(&zeroes)?;

        Ok(Self {
            version,
            sections: Vec::new(),
            page_records: Vec::new(),
            crc_seed: 0,
            next_page_id: 1,
            skip_lz77: false,
        })
    }

    /// Get the file offset where the AcDbObjects section starts.
    ///
    /// For AC21, handles are relative to decompressed section data (always 0).
    pub fn handle_section_offset(&self) -> usize {
        0
    }

    /// Add a section to the file.
    ///
    /// The section data is partitioned into pages based on the section's
    /// spec-defined page size, each page is optionally compressed, RS-encoded,
    /// and written to the output stream.
    pub fn add_section<W: Write + Seek>(
        &mut self,
        output: &mut W,
        name: &str,
        data: &[u8],
    ) -> Result<(), DxfError> {
        let hash_code = ac21_section_info::hash_code(name)
            .ok_or_else(|| DxfError::InvalidFormat(
                format!("Unknown AC21 section: {}", name),
            ))?;
        let encoding = ac21_section_info::encoding(name).unwrap_or(1);
        // We don't implement XOR obfuscation (encryption=2), so always
        // store encryption=0 in the section map.  AutoCAD will read the
        // data without attempting to decrypt, which is correct.
        let encryption: u64 = 0;
        let max_page_size = ac21_section_info::page_size(name)
            .unwrap_or(0xF800); // Default for variable-size sections

        let mut section = AC21SectionInfo {
            name: name.to_string(),
            hash_code,
            encoding,
            encryption,
            max_page_size,
            data_size: data.len() as u64,
            pages: Vec::new(),
        };

        // Partition data into pages
        let page_size = max_page_size as usize;
        let mut offset: usize = 0;

        while offset < data.len() {
            let remaining = data.len() - offset;
            let chunk_size = remaining.min(page_size);
            let chunk = &data[offset..offset + chunk_size];

            let page_record = self.write_data_page(
                output,
                chunk,
                encoding,
                offset as u64,
                max_page_size,
            )?;

            section.pages.push(page_record);
            offset += chunk_size;
        }

        self.sections.push(section);
        Ok(())
    }

    /// Finalize the file: write section map, page map, file header, metadata.
    ///
    /// ## File layout (spec §5.1):
    /// ```text
    /// [0x80  Metadata]
    /// [0x400 File header]
    /// [0x400 Page map (system page)]
    /// [0x400 Page map copy]
    /// [Data section pages ...]
    /// [Section map (system page)]
    /// [Section map copy]
    /// [0x400 File header copy]
    /// ```
    pub fn write_file<W: Write + Seek>(
        &mut self,
        output: &mut W,
    ) -> Result<(), DxfError> {
        // Initialize CRC seed (0 matches AutoCAD reference files)
        self.crc_seed = 0;
        let mut rng = CrcRandomEncoder::new(self.crc_seed);

        // ── Step 1: Write section map + copy (appended after data pages) ──
        let section_map_data = self.build_section_map()?;
        let section_map_result = self.write_system_page(output, &section_map_data)?;
        let section_map2_result = self.write_system_page(output, &section_map_data)?;

        // ── Step 2: Build page map ──
        // Assign IDs for the page map pages (written at reserved positions)
        let page_map_page_id = self.next_page_id;
        self.next_page_id += 1;
        let page_map2_page_id = self.next_page_id;
        self.next_page_id += 1;

        let pm_page_size = MIN_SYSTEM_PAGE_SIZE as i64; // 0x400

        // Build page map with entries in file-offset order:
        // [page_map, page_map_copy, data_pages..., section_map_pages...]
        let page_map_data = self.build_page_map_ordered(
            page_map_page_id, page_map2_page_id, pm_page_size,
        );

        // ── Step 3: Write page map at reserved positions ──
        let page_map_result = self.write_system_page_at(
            output, &page_map_data,
            RESERVED_HEADER_SIZE as u64,  // absolute 0x480
            page_map_page_id,
            MIN_SYSTEM_PAGE_SIZE,
        )?;
        let _page_map2_result = self.write_system_page_at(
            output, &page_map_data,
            (RESERVED_HEADER_SIZE + MIN_SYSTEM_PAGE_SIZE) as u64,  // absolute 0x880
            page_map2_page_id,
            MIN_SYSTEM_PAGE_SIZE,
        )?;

        // Seek back to end for the file header copy
        output.seek(SeekFrom::End(0))?;

        // ── Step 4: Build compressed metadata ──
        let mut metadata = Dwg21CompressedMetadata::default();
        metadata.file_size = 0; // Patched after writing header copy

        // Pages map fields — page map is at relative offset 0
        metadata.pages_map_crc_compressed = page_map_result.crc_compressed;
        metadata.pages_map_correction_factor = page_map_result.correction_factor;
        metadata.pages_map_crc_seed = self.crc_seed;
        metadata.pages_map_offset = 0; // Page map is at data start
        metadata.pages_map_id = page_map_page_id as u64;
        metadata.map2_offset = MIN_SYSTEM_PAGE_SIZE as u64; // 0x400
        metadata.map2_id = page_map2_page_id as u64;
        metadata.pages_map_size_compressed = page_map_result.compressed_size;
        metadata.pages_map_size_uncompressed = page_map_result.uncompressed_size;
        // Total pages = data pages + section map pages + page map pages
        let total_pages = self.page_records.len() as u64 + 2; // +2 for page map pages
        metadata.pages_amount = total_pages;
        metadata.pages_max_id = (self.next_page_id - 1) as u64;
        metadata.pages_map_crc_uncompressed = page_map_result.crc_uncompressed;

        // Sections map fields
        metadata.sections_amount = (self.sections.len() + 1) as u64; // +1 per spec
        metadata.sections_map_crc_uncompressed = section_map_result.crc_uncompressed;
        metadata.sections_map_size_compressed = section_map_result.compressed_size;
        metadata.sections_map2_id = section_map2_result.page_id as u64;
        metadata.sections_map_id = section_map_result.page_id as u64;
        metadata.sections_map_size_uncompressed = section_map_result.uncompressed_size;
        metadata.sections_map_crc_compressed = section_map_result.crc_compressed;
        metadata.sections_map_correction_factor = section_map_result.correction_factor;
        metadata.sections_map_crc_seed = self.crc_seed;

        // CRC/random fields
        metadata.crc_seed = self.crc_seed;
        metadata.crc_seed_encoded = rng.encode_crc_seed(self.crc_seed);
        metadata.random_seed = rng.next_u64();

        // Compute header CRC-64 (spec §5.2.1.2)
        let meta_bytes = metadata.to_bytes();
        let header_crc = dwg_ac21_header_crc64(&meta_bytes);
        metadata.header_crc64 = header_crc;

        // ── Step 5: Build file header page ──
        let file_header_page = self.build_file_header_page(&metadata, &mut rng)?;
        debug_assert_eq!(file_header_page.len(), FILE_HEADER_PAGE_SIZE);

        // Write file header at offset 0x80
        output.seek(SeekFrom::Start(METADATA_BLOCK_SIZE as u64))?;
        output.write_all(&file_header_page)?;

        // Write file header copy at end of file
        output.seek(SeekFrom::End(0))?;
        let header2_offset = output.seek(SeekFrom::Current(0))?;
        output.write_all(&file_header_page)?;

        // ── Step 6: Patch file size and header2_offset ──
        let file_size = output.seek(SeekFrom::Current(0))?;
        metadata.file_size = file_size;
        metadata.header2_offset = header2_offset - RESERVED_HEADER_SIZE as u64;

        // Recompute with updated metadata
        let mut rng2 = CrcRandomEncoder::new(self.crc_seed);
        metadata.crc_seed_encoded = rng2.encode_crc_seed(self.crc_seed);
        metadata.random_seed = rng2.next_u64();
        let meta_bytes = metadata.to_bytes();
        let header_crc = dwg_ac21_header_crc64(&meta_bytes);
        metadata.header_crc64 = header_crc;

        let file_header_page = self.build_file_header_page(&metadata, &mut rng2)?;

        // Overwrite file header at 0x80
        output.seek(SeekFrom::Start(METADATA_BLOCK_SIZE as u64))?;
        output.write_all(&file_header_page)?;

        // Overwrite file header copy at end
        output.seek(SeekFrom::Start(header2_offset))?;
        output.write_all(&file_header_page)?;

        // ── Step 7: Write metadata at offset 0x00 ──
        self.write_metadata(output, &metadata)?;

        // Seek to end
        output.seek(SeekFrom::End(0))?;

        Ok(())
    }

    // ── Data page writing ──

    /// Write a single data page to the output stream.
    ///
    /// For encoding=1 (stored): writes raw data with 32-byte alignment.
    /// For encoding=4 (compressed): LZ77 compress → RS-encode → 32-byte align.
    fn write_data_page<W: Write + Seek>(
        &mut self,
        output: &mut W,
        data: &[u8],
        encoding: u64,
        data_offset: u64,
        max_page_size: u64,
    ) -> Result<AC21SectionPageRecord, DxfError> {
        let page_id = self.next_page_id;
        self.next_page_id += 1;

        let checksum = dwg_ac21_page_checksum(0, data) as u64;

        if encoding == 1 {
            // encoding=1: Store raw data without RS encoding.
            // Analysis of AutoCAD-generated R2007 files shows encoding=1 pages
            // are stored as raw (uncompressed, un-RS-encoded) data aligned to
            // 32-byte boundaries.  The encoding field means "no LZ77" and also
            // "no RS".  AutoCAD reads these pages directly without RS decoding.
            let page_crc = dwg_ac21_mirrored_crc64(0, data.len() as u32, data);

            let aligned_size = align32(data.len());
            let offset = output.seek(SeekFrom::Current(0))?;
            output.write_all(data)?;
            // Pad to 32-byte alignment
            let padding = aligned_size - data.len();
            if padding > 0 {
                output.write_all(&vec![0u8; padding])?;
            }

            self.page_records.push(AC21PageRecord {
                id: page_id,
                size: aligned_size as i64,
                offset,
            });

            Ok(AC21SectionPageRecord {
                data_offset,
                page_size: max_page_size,
                page_id,
                uncompressed_size: data.len() as u64,
                compressed_size: data.len() as u64,
                checksum,
                crc: page_crc,
            })
        } else {
            // encoding=4: compress + RS encode
            let compressed = compress_ac21(data);
            // Use raw data if compression doesn't reduce size (matches AutoCAD behavior).
            // The reader checks compressed_size == uncompressed_size to skip decompression.
            // Also skip compression if skip_lz77 flag is set (for debugging).
            let (page_data, comp_size) = if self.skip_lz77 || compressed.len() >= data.len() {
                (data, data.len() as u64)
            } else {
                (compressed.as_slice(), compressed.len() as u64)
            };
            // CRC-64 is computed on the compressed page data BEFORE RS encoding (spec §5.4).
            let page_crc = dwg_ac21_mirrored_crc64(0, page_data.len() as u32, page_data);
            let encoded = rs_encode_data_page_interleaved(page_data, encoding);

            // Align to 32-byte boundary
            let aligned_size = align32(encoded.len());
            let offset = output.seek(SeekFrom::Current(0))?;
            output.write_all(&encoded)?;
            let padding = aligned_size - encoded.len();
            if padding > 0 {
                output.write_all(&vec![0u8; padding])?;
            }

            self.page_records.push(AC21PageRecord {
                id: page_id,
                size: aligned_size as i64,
                offset,
            });

            Ok(AC21SectionPageRecord {
                data_offset,
                page_size: max_page_size,
                page_id,
                uncompressed_size: data.len() as u64,
                compressed_size: comp_size,
                checksum,
                crc: page_crc,
            })
        }
    }

    // ── System page writing ──

    /// Compress and RS-encode system page data.
    ///
    /// The `target_page_size` determines the correction factor: we compute the
    /// maximum correction factor that fills the page without exceeding it.
    /// This matches how AutoCAD/ACadSharp compute the factor (spec §5.3).
    ///
    /// Returns the encoded bytes and metadata (does NOT write to stream).
    fn encode_system_page(
        &self,
        data: &[u8],
        target_page_size: usize,
    ) -> (Vec<u8>, u64, u64, u64, u64, u64) {
        // CRCs on uncompressed data (Mirrored CRC-64 per spec §5.3, §7.2)
        let uncompressed_size = data.len() as u64;
        let crc_uncompressed = dwg_ac21_mirrored_crc64(
            self.crc_seed,
            data.len() as u32,
            data,
        );

        // Compress
        let compressed = compress_ac21(data);
        let use_compressed = compressed.len() < data.len();
        let page_data = if use_compressed { &compressed } else { data };
        let compressed_size = if use_compressed {
            compressed.len() as u64
        } else {
            data.len() as u64
        };

        // Compressed CRC (Mirrored CRC-64 per spec §5.3, §7.2)
        let crc_compressed = dwg_ac21_mirrored_crc64(
            self.crc_seed,
            page_data.len() as u32,
            page_data,
        );

        // Compute correction factor to fill the target page size optimally.
        // max_factor = max RS blocks that fit in the page
        // correction_factor = floor(max_factor * RS_SYSTEM_K / align8(compressed_size))
        // This maximizes error correction redundancy, matching ref file behavior.
        let aligned_comp = ((page_data.len() as u64 + 7) & !7) as usize;
        let correction_factor: u64 = if aligned_comp == 0 {
            1
        } else {
            let max_rs_blocks = target_page_size / RS_N; // floor(target / 255)
            let max_data_capacity = max_rs_blocks * RS_SYSTEM_K;
            let cf = max_data_capacity / aligned_comp;
            cf.max(2) as u64 // minimum correction factor of 2
        };
        let total_size = aligned_comp * correction_factor as usize;
        let factor = (total_size + RS_SYSTEM_K - 1) / RS_SYSTEM_K;

        // RS-encode with RS(255, 239)
        let mut padded_data = vec![0u8; factor * RS_SYSTEM_K];
        let copy_len = page_data.len().min(padded_data.len());
        padded_data[..copy_len].copy_from_slice(&page_data[..copy_len]);

        let mut encoded = vec![0u8; factor * RS_N];
        reed_solomon_encode(
            &padded_data,
            &mut encoded,
            factor,
            RS_SYSTEM_K,
            RS_SYSTEM_PRIM_POLY,
        );

        (encoded, compressed_size, uncompressed_size, crc_compressed, crc_uncompressed, correction_factor)
    }

    /// Write a system page (section map) per spec §5.3.
    ///
    /// Page size is computed from the uncompressed data size per §5.3.1,
    /// ensuring the RS-encoded data fits the page at least 2 times.
    fn write_system_page<W: Write + Seek>(
        &mut self,
        output: &mut W,
        data: &[u8],
    ) -> Result<SystemPageResult, DxfError> {
        let page_id = self.next_page_id;
        self.next_page_id += 1;

        // Compute page size from uncompressed data size per spec §5.3.1
        let target_page_size = get_system_page_size(data.len() as u64) as usize;
        let (encoded, compressed_size, uncompressed_size, crc_compressed, crc_uncompressed, correction_factor)
            = self.encode_system_page(data, target_page_size);

        // Page size must accommodate the encoded data
        let page_size = align32(encoded.len()).max(target_page_size);

        let offset = output.seek(SeekFrom::Current(0))?;
        output.write_all(&encoded)?;
        let padding = page_size - encoded.len();
        if padding > 0 {
            output.write_all(&vec![0u8; padding])?;
        }

        self.page_records.push(AC21PageRecord {
            id: page_id,
            size: page_size as i64,
            offset,
        });

        Ok(SystemPageResult {
            page_id,
            offset,
            compressed_size,
            uncompressed_size,
            crc_compressed,
            crc_uncompressed,
            correction_factor,
        })
    }

    /// Write a system page at a specific absolute position (for page map).
    ///
    /// Unlike `write_system_page`, this seeks to `abs_position` and does NOT
    /// add to `page_records` (caller handles page map entries separately).
    fn write_system_page_at<W: Write + Seek>(
        &self,
        output: &mut W,
        data: &[u8],
        abs_position: u64,
        page_id: i64,
        target_size: usize,
    ) -> Result<SystemPageResult, DxfError> {
        let (encoded, compressed_size, uncompressed_size, crc_compressed, crc_uncompressed, correction_factor)
            = self.encode_system_page(data, target_size);

        assert!(
            encoded.len() <= target_size,
            "RS-encoded page map ({} bytes) exceeds target size ({} bytes)",
            encoded.len(), target_size,
        );

        output.seek(SeekFrom::Start(abs_position))?;
        output.write_all(&encoded)?;
        let padding = target_size - encoded.len();
        if padding > 0 {
            output.write_all(&vec![0u8; padding])?;
        }

        Ok(SystemPageResult {
            page_id,
            offset: abs_position,
            compressed_size,
            uncompressed_size,
            crc_compressed,
            crc_uncompressed,
            correction_factor,
        })
    }

    // ── Section map building ──

    /// Build the section map data (spec §5.2 section map format).
    ///
    /// Entries are emitted in the spec-mandated section map order
    /// (`SECTION_MAP_ORDER`), which differs from the physical stream order.
    /// For each section: 0x40-byte header + UTF-16LE name + per-page records.
    fn build_section_map(&self) -> Result<Vec<u8>, DxfError> {
        let mut stream = Vec::new();

        // Build index mapping section name → position in spec section-map order.
        let map_order = ac21_section_info::SECTION_MAP_ORDER;

        // Sort sections by the spec section-map order.
        let mut sorted: Vec<&AC21SectionInfo> = self.sections.iter().collect();
        sorted.sort_by_key(|s| {
            map_order.iter().position(|&n| n == s.name).unwrap_or(usize::MAX)
        });

        for section in &sorted {
            // Section header (8 fields × 8 bytes = 0x40 bytes)

            // 0x00: Data size (8 bytes)
            stream.write_u64::<LittleEndian>(section.data_size)?;
            // 0x08: Max size / page size (8 bytes)
            stream.write_u64::<LittleEndian>(section.max_page_size)?;
            // 0x10: Encryption (8 bytes)
            stream.write_u64::<LittleEndian>(section.encryption)?;
            // 0x18: HashCode (8 bytes)
            stream.write_u64::<LittleEndian>(section.hash_code as u64)?;
            // 0x20: SectionNameLength (8 bytes) — byte count of name data
            //   The reader reads exactly this many bytes, so it must be the
            //   total UTF-16LE byte count including the null terminator.
            let name_chars: Vec<u16> = section.name.encode_utf16().collect();
            let name_byte_len = if name_chars.is_empty() {
                0u64
            } else {
                (name_chars.len() as u64 + 1) * 2  // chars + null, each 2 bytes
            };
            stream.write_u64::<LittleEndian>(name_byte_len)?;
            // 0x28: Unknown (8 bytes)
            stream.write_u64::<LittleEndian>(0)?;
            // 0x30: Encoding (8 bytes)
            stream.write_u64::<LittleEndian>(section.encoding)?;
            // 0x38: NumPages (8 bytes)
            stream.write_u64::<LittleEndian>(section.pages.len() as u64)?;

            // 0x40: UTF-16LE section name + null terminator (if name_len > 0)
            if !name_chars.is_empty() {
                for &ch in &name_chars {
                    stream.write_u16::<LittleEndian>(ch)?;
                }
                // Null terminator (2 bytes)
                stream.write_u16::<LittleEndian>(0)?;
            }

            // Per-page records (7 × u64 = 56 bytes each)
            for page in &section.pages {
                stream.write_u64::<LittleEndian>(page.data_offset)?;
                stream.write_u64::<LittleEndian>(page.page_size)?;
                stream.write_u64::<LittleEndian>(page.page_id as u64)?;
                stream.write_u64::<LittleEndian>(page.uncompressed_size)?;
                stream.write_u64::<LittleEndian>(page.compressed_size)?;
                stream.write_u64::<LittleEndian>(page.checksum)?;
                stream.write_u64::<LittleEndian>(page.crc)?;
            }
        }

        Ok(stream)
    }

    // ── Page map building ──

    /// Build the page map data (spec §5.2 page map format).
    ///
    /// Entries are ordered by file position:
    /// 1. Page map pages (at offsets 0 and 0x400 relative to data start)
    /// 2. Data pages (from self.page_records, in write order)
    /// 3. Section map pages (also in self.page_records, after data pages)
    ///
    /// Each entry is (size: i64, id: i64).
    fn build_page_map_ordered(
        &self,
        page_map_id: i64,
        page_map2_id: i64,
        page_map_page_size: i64,
    ) -> Vec<u8> {
        // +2 for page map pages, +1 for null terminator
        let capacity = (self.page_records.len() + 3) * 16;
        let mut stream = Vec::with_capacity(capacity);

        // 1. Page map pages come first (they're at the lowest file offsets)
        stream.extend_from_slice(&page_map_page_size.to_le_bytes());
        stream.extend_from_slice(&page_map_id.to_le_bytes());
        stream.extend_from_slice(&page_map_page_size.to_le_bytes());
        stream.extend_from_slice(&page_map2_id.to_le_bytes());

        // 2. Data pages + section map pages (already in file-offset order)
        for record in &self.page_records {
            stream.extend_from_slice(&record.size.to_le_bytes());
            stream.extend_from_slice(&record.id.to_le_bytes());
        }

        // 3. Null terminator (size=0, id=0) per spec §5.3
        stream.extend_from_slice(&0i64.to_le_bytes());
        stream.extend_from_slice(&0i64.to_le_bytes());

        stream
    }

    // ── File header page building (spec §5.2.1) ──

    /// Build the 0x400-byte file header page (spec §5.2.1 steps 1-7).
    fn build_file_header_page(
        &self,
        metadata: &Dwg21CompressedMetadata,
        rng: &mut CrcRandomEncoder,
    ) -> Result<Vec<u8>, DxfError> {
        // Step 1: Generate check data (spec §5.2.1.1)
        let random1 = rng.next_u64();
        let random2 = rng.next_u64();
        let encoded_crc_seed = rng.encode_crc_seed(self.crc_seed);

        // Normal CRC (spec §5.2.1.1)
        let normal_crc = {
            let mut buf = [0u64; 8];
            buf[0] = encode_value(random1, random2);
            buf[1] = encode_value(buf[0], buf[0]);
            buf[2] = encode_value(random2, buf[1]);
            buf[3] = encode_value(buf[2], buf[2]);
            buf[4] = encode_value(random1, buf[3]);
            buf[5] = encode_value(buf[4], buf[4]);
            buf[6] = encode_value(buf[5], buf[5]);
            buf[7] = encode_value(buf[6], buf[6]);

            let mut bytes = [0u8; 64];
            for (i, &val) in buf.iter().enumerate() {
                bytes[i * 8..(i + 1) * 8].copy_from_slice(&val.to_le_bytes());
            }
            dwg_ac21_normal_crc64(random2, 64, &bytes)
        };

        // Mirrored CRC (spec §5.2.1.1)
        let mirrored_crc = {
            let mut buf = [0u64; 8];
            buf[0] = encode_value(random1, random2);
            buf[1] = encode_value(normal_crc, buf[0]);
            buf[2] = encode_value(random2, buf[1]);
            buf[3] = encode_value(normal_crc, buf[2]);
            buf[4] = encode_value(random1, buf[3]);
            buf[5] = encode_value(normal_crc, buf[4]);
            buf[6] = encode_value(random2, buf[5]);
            buf[7] = encode_value(buf[6], buf[6]);

            let mut bytes = [0u8; 64];
            for (i, &val) in buf.iter().enumerate() {
                bytes[i * 8..(i + 1) * 8].copy_from_slice(&val.to_le_bytes());
            }
            dwg_ac21_mirrored_crc64(random1, 64, &bytes)
        };

        // Check data stream layout (spec §5.2.1.1 stream position annotations):
        // 1st in stream: Normal CRC
        // 2nd in stream: Mirrored CRC
        // 3rd in stream: Random value 1
        // 4th in stream: Random value 2
        // 5th in stream: Encoded CRC Seed
        // Written at position 0x3D8 (§5.2.1.7)
        let mut check_data = [0u8; CHECK_DATA_SIZE];
        check_data[0..8].copy_from_slice(&normal_crc.to_le_bytes());
        check_data[8..16].copy_from_slice(&mirrored_crc.to_le_bytes());
        check_data[16..24].copy_from_slice(&random1.to_le_bytes());
        check_data[24..32].copy_from_slice(&random2.to_le_bytes());
        check_data[32..40].copy_from_slice(&encoded_crc_seed.to_le_bytes());

        // Step 2: Serialize metadata and compute header CRC-64 (already done in caller)
        let meta_bytes = metadata.to_bytes();
        debug_assert_eq!(meta_bytes.len(), METADATA_SIZE);

        // Step 3: Compress metadata (spec §5.2.1.3)
        let compressed = compress_ac21(&meta_bytes);
        let use_compressed = compressed.len() < meta_bytes.len();
        let compr_data = if use_compressed { &compressed } else { &meta_bytes };
        let compr_len = if use_compressed {
            compr_data.len() as i32
        } else {
            -(meta_bytes.len() as i32)
        };

        // Compressed CRC (spec §5.2.1.3)
        let compr_crc = dwg_ac21_normal_crc64(0, compr_data.len() as u32, compr_data);

        // Step 4: Checking sequence (spec §5.2.1.4)
        let check_seq_val1 = rng.next_u64();
        let check_seq_val2 = encode_value(check_seq_val1, check_seq_val1);

        let mut check_seq_bytes = [0u8; 16];
        check_seq_bytes[0..8].copy_from_slice(&check_seq_val1.to_le_bytes());
        check_seq_bytes[8..16].copy_from_slice(&check_seq_val2.to_le_bytes());

        let check_seq_crc = dwg_ac21_normal_crc64_seed1(0, 16, &check_seq_bytes);

        // Step 5: Build pre-RS buffer (3 × 239 bytes) (spec §5.2.1.5)
        let total_rs_data = FILE_HEADER_RS_FACTOR * RS_SYSTEM_K; // 3 × 239 = 717
        let mut pre_rs = vec![0u8; total_rs_data];

        // Build the inner block
        let mut block = Vec::with_capacity(64);
        // 0x00: Checking sequence CRC (8 bytes)
        block.extend_from_slice(&check_seq_crc.to_le_bytes());
        // 0x08: Checking sequence val1 (8 bytes)
        block.extend_from_slice(&check_seq_val1.to_le_bytes());
        // 0x10: Compressed data CRC (8 bytes)
        block.extend_from_slice(&compr_crc.to_le_bytes());
        // 0x18: Compressed data size (4 bytes) + Length2 (4 bytes)
        block.extend_from_slice(&compr_len.to_le_bytes());
        block.extend_from_slice(&0i32.to_le_bytes()); // Length2 = 0 per spec/reference
        // 0x20: Compressed (or raw) data
        block.extend_from_slice(compr_data);

        // Pad block to multiple of 8
        let padded_block_size = (block.len() + 7) & !7;
        while block.len() < padded_block_size {
            let b = rng.next_u32() as u8;
            block.push(b);
        }

        // Write block once at offset 0, fill remaining with random padding (spec §5.2.1.5)
        pre_rs[..block.len()].copy_from_slice(&block);
        if block.len() < total_rs_data {
            rng.fill_random(&mut pre_rs[block.len()..total_rs_data]);
        }

        // Step 6: RS-encode with RS(255, 239) factor=3 (spec §5.2.1.6)
        let rs_encoded_size = FILE_HEADER_RS_FACTOR * RS_N; // 3 × 255 = 765
        let mut rs_encoded = vec![0u8; rs_encoded_size];
        reed_solomon_encode(
            &pre_rs,
            &mut rs_encoded,
            FILE_HEADER_RS_FACTOR,
            RS_SYSTEM_K,
            RS_SYSTEM_PRIM_POLY,
        );

        // Build the full 0x400-byte page
        let mut page = vec![0u8; FILE_HEADER_PAGE_SIZE];
        // Copy RS-encoded data
        let copy_len = rs_encoded_size.min(RS_DATA_IN_HEADER);
        page[..copy_len].copy_from_slice(&rs_encoded[..copy_len]);

        // Fill remaining space (between RS data and check data) with random padding
        if copy_len < RS_DATA_IN_HEADER {
            rng.fill_random(&mut page[copy_len..RS_DATA_IN_HEADER]);
        }

        // Step 7: Overwrite last 0x28 bytes with check data (spec §5.2.1.7)
        page[RS_DATA_IN_HEADER..FILE_HEADER_PAGE_SIZE]
            .copy_from_slice(&check_data);

        Ok(page)
    }

    // ── Metadata writing ──

    /// Write the 0x80-byte metadata block at offset 0x00 (spec §5.2).
    fn write_metadata<W: Write + Seek>(
        &self,
        output: &mut W,
        _metadata: &Dwg21CompressedMetadata,
    ) -> Result<(), DxfError> {
        output.seek(SeekFrom::Start(0))?;

        let mut buf = [0u8; METADATA_BLOCK_SIZE];
        {
            let mut cursor = Cursor::new(&mut buf[..]);

            // 0x00: "AC1021" version string (6 bytes)
            cursor.write_all(self.version.as_str().as_bytes())?;

            // 0x06: 5 bytes of 0x00
            cursor.write_all(&[0u8; 5])?;

            // 0x0B: Maintenance release version (0x19 matches AutoCAD 2007)
            cursor.write_all(&[0x19u8])?;

            // 0x0C: Byte 0x03 (constant, matches AC18 and AutoCAD reference files)
            cursor.write_all(&[0x03u8])?;

            // 0x0D: Preview address (4 bytes)
            // Find Preview section page offset
            let preview_addr = self.find_section_page_offset(names::PREVIEW);
            cursor.write_u32::<LittleEndian>(preview_addr)?;

            // 0x11: DWG version byte (writer application version)
            cursor.write_all(&[0x1Bu8])?;

            // 0x12: Maintenance release version (app)
            cursor.write_all(&[0x19u8])?;

            // 0x13: Codepage (2 bytes)
            cursor.write_u16::<LittleEndian>(30)?; // ANSI_1252

            // 0x15: 3 zero bytes
            cursor.write_all(&[0u8; 3])?;

            // 0x18: SecurityType (4 bytes)
            cursor.write_i32::<LittleEndian>(0)?;

            // 0x1C: Unknown long (4 bytes)
            cursor.write_i32::<LittleEndian>(0)?;

            // 0x20: Summary info address (4 bytes)
            let summary_addr = self.find_section_page_offset(names::SUMMARY_INFO);
            cursor.write_u32::<LittleEndian>(summary_addr)?;

            // 0x24: VBA Project address (4 bytes, 0 if not present)
            let vba_addr = self.find_section_page_offset(names::VBA_PROJECT);
            cursor.write_u32::<LittleEndian>(vba_addr)?;

            // 0x28: 0x00000080 (4 bytes)
            cursor.write_u32::<LittleEndian>(0x80)?;

            // 0x2C: App info address (4 bytes)
            let app_info_addr = self.find_section_page_offset(names::APP_INFO);
            cursor.write_u32::<LittleEndian>(app_info_addr)?;

            // 0x30..0x80: remaining zeros (already zeroed)
        }

        output.write_all(&buf)?;

        Ok(())
    }

    /// Find the file offset of the first page of a named section.
    fn find_section_page_offset(&self, name: &str) -> u32 {
        for section in &self.sections {
            if section.name == name {
                if let Some(first_page) = section.pages.first() {
                    // Look up the page record to get its file offset
                    for record in &self.page_records {
                        if record.id == first_page.page_id {
                            return record.offset as u32;
                        }
                    }
                }
            }
        }
        0
    }
}

/// Result from writing a system page, containing metadata for the file header.
#[derive(Debug)]
struct SystemPageResult {
    /// Page ID assigned to this system page.
    page_id: i64,
    /// Absolute file offset where the page was written.
    #[allow(dead_code)]
    offset: u64,
    /// Compressed size of the data.
    compressed_size: u64,
    /// Uncompressed size of the data.
    uncompressed_size: u64,
    /// CRC-64 of compressed data.
    crc_compressed: u64,
    /// CRC-64 of uncompressed data.
    crc_uncompressed: u64,
    /// RS correction factor (number of interleaved sub-streams).
    correction_factor: u64,
}

// ════════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ─── CRC Random Encoder tests ───────────────────────────────────

    #[test]
    fn test_crc_random_encoder_deterministic() {
        let mut rng1 = CrcRandomEncoder::new(42);
        let mut rng2 = CrcRandomEncoder::new(42);

        for _ in 0..100 {
            assert_eq!(rng1.next_u32(), rng2.next_u32());
        }
    }

    #[test]
    fn test_crc_random_encoder_different_seeds() {
        let mut rng1 = CrcRandomEncoder::new(0x12345678_9ABCDEF0);
        let mut rng2 = CrcRandomEncoder::new(0xFEDCBA98_76543210);

        // Different seeds should produce different sequences
        let seq1: Vec<u32> = (0..10).map(|_| rng1.next_u32()).collect();
        let seq2: Vec<u32> = (0..10).map(|_| rng2.next_u32()).collect();
        assert_ne!(seq1, seq2);
    }

    #[test]
    fn test_crc_random_encoder_fill_random() {
        let mut rng = CrcRandomEncoder::new(123);
        let mut buf = [0u8; 37]; // Non-aligned size
        rng.fill_random(&mut buf);

        // Should not be all zeros
        assert!(!buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_crc_random_encoder_encode_seed() {
        let mut rng = CrcRandomEncoder::new(42);
        let seed = 0x12345678_9ABCDEF0u64;
        let encoded = rng.encode_crc_seed(seed);

        // With XOR obfuscation, the encoded value should DIFFER from the seed
        // (the random bits XOR with seed bits, producing a different value)
        assert_ne!(encoded, seed, "Encoded value should differ from seed due to XOR");

        // The encoding should be deterministic (same seed + RNG state = same result)
        let mut rng2 = CrcRandomEncoder::new(42);
        let encoded2 = rng2.encode_crc_seed(seed);
        assert_eq!(encoded, encoded2, "Same RNG state should produce same encoding");
    }

    #[test]
    fn test_crc_random_encoder_encode_seed_advances_state() {
        let mut rng1 = CrcRandomEncoder::new(42);
        let mut rng2 = CrcRandomEncoder::new(42);

        // encode_crc_seed consumes 7 u32 values
        rng1.encode_crc_seed(0x12345678_9ABCDEF0);
        for _ in 0..7 {
            rng2.next_u32();
        }
        // After 7 consumptions, both RNGs should be in the same state
        assert_eq!(rng1.next_u32(), rng2.next_u32());
        assert_eq!(rng1.next_u32(), rng2.next_u32());
    }

    // ─── System page size calculation tests ─────────────────────────

    #[test]
    fn test_system_page_size_minimum() {
        // Small data should give minimum page size of 0x400
        assert_eq!(get_system_page_size(8), 0x400);
        assert_eq!(get_system_page_size(100), 0x400);
        assert_eq!(get_system_page_size(200), 0x400);
    }

    #[test]
    fn test_system_page_size_large_data() {
        // Large enough data should produce a larger page size
        let size = get_system_page_size(10000);
        assert!(size >= 10000 * 2 / RS_SYSTEM_K as u64 * RS_N as u64);
        // Must be 0x20-aligned
        assert_eq!(size % PAGE_ALIGN_SIZE as u64, 0);
    }

    #[test]
    fn test_system_page_size_alignment() {
        // Result should always be 0x20-aligned or exactly 0x400
        for data_size in [8, 100, 500, 1000, 5000, 10000, 50000u64] {
            let ps = get_system_page_size(data_size);
            assert!(
                ps == 0x400 || ps % PAGE_ALIGN_SIZE as u64 == 0,
                "Page size {ps:#X} for data_size {data_size} not aligned"
            );
        }
    }

    // ─── Encode value tests ─────────────────────────────────────────

    #[test]
    fn test_encode_value_zero_shift() {
        assert_eq!(encode_value(0x1234, 0), 0x1234);
        // shift = 0 & 0x1F = 0 → no rotation
        assert_eq!(encode_value(0x1234, 0x20), 0x1234);
    }

    #[test]
    fn test_encode_value_shift_one() {
        // shift = 1 → rotate left by 1
        let result = encode_value(0x8000_0000_0000_0001, 1);
        assert_eq!(result, 0x0000_0000_0000_0003);
    }

    // ─── Writer structure tests ─────────────────────────────────────

    #[test]
    fn test_new_reserves_header_space() {
        let mut output = Cursor::new(Vec::new());
        let _writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        assert_eq!(output.position(), TOTAL_RESERVED_SIZE as u64);
        assert_eq!(output.get_ref().len(), TOTAL_RESERVED_SIZE);
        assert!(output.get_ref().iter().all(|&b| b == 0));
    }

    #[test]
    fn test_handle_section_offset_always_zero() {
        let mut output = Cursor::new(Vec::new());
        let writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();
        assert_eq!(writer.handle_section_offset(), 0);
    }

    #[test]
    fn test_add_section_creates_pages() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        let data = vec![0xAA; 100];
        writer.add_section(&mut output, names::HEADER, &data).unwrap();

        assert_eq!(writer.sections.len(), 1);
        assert_eq!(writer.sections[0].pages.len(), 1);
        assert_eq!(writer.page_records.len(), 1);
    }

    #[test]
    fn test_add_section_multi_page() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        // AcDb:Header has page_size 0x800 = 2048
        // Data larger than 2 pages
        let data = vec![0xBB; 0x800 * 2 + 100];
        writer.add_section(&mut output, names::HEADER, &data).unwrap();

        assert_eq!(writer.sections.len(), 1);
        assert_eq!(writer.sections[0].pages.len(), 3);
        assert_eq!(writer.page_records.len(), 3);
    }

    #[test]
    fn test_add_multiple_sections() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        writer.add_section(&mut output, names::HEADER, &vec![0xAA; 100]).unwrap();
        writer.add_section(&mut output, names::CLASSES, &vec![0xBB; 200]).unwrap();

        assert_eq!(writer.sections.len(), 2);
        assert_eq!(writer.sections[0].name, names::HEADER);
        assert_eq!(writer.sections[1].name, names::CLASSES);
    }

    #[test]
    fn test_add_section_unknown_name_fails() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        let result = writer.add_section(&mut output, "AcDb:Nonexistent", &vec![0; 10]);
        assert!(result.is_err());
    }

    // ─── Section map building tests ─────────────────────────────────

    #[test]
    fn test_build_section_map_format() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        writer.add_section(&mut output, names::HEADER, &vec![0xAA; 100]).unwrap();

        let map_data = writer.build_section_map().unwrap();

        // Should have: 0x40 header + UTF-16LE name + null term + page records
        // "AcDb:Header" = 11 chars → (11 + 1) × 2 = 24 bytes for name data
        // 1 page → 7 × 8 = 56 bytes
        // Total: 0x40 + 24 + 56 = 144 bytes
        assert_eq!(map_data.len(), 0x40 + 24 + 56);

        // Verify hash code at offset 0x18
        let hash = u64::from_le_bytes(map_data[0x18..0x20].try_into().unwrap());
        assert_eq!(hash, 0x32B803D9); // AcDb:Header hash code

        // Verify SectionNameLength at offset 0x20 is byte count (not char count)
        // "AcDb:Header" = 11 chars + 1 null = 12 × 2 = 24 bytes
        let name_len = u64::from_le_bytes(map_data[0x20..0x28].try_into().unwrap());
        assert_eq!(name_len, 24, "SectionNameLength should be byte count including null terminator");
    }

    #[test]
    fn test_section_map_name_readable_by_reader() {
        // Verify that the section map name data can be parsed the same way
        // the reader does: read name_length bytes, decode as UTF-16LE, trim nulls
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        writer.add_section(&mut output, names::CLASSES, &vec![0xBB; 50]).unwrap();

        let map_data = writer.build_section_map().unwrap();

        // Read name_length from offset 0x20
        let name_len = u64::from_le_bytes(map_data[0x20..0x28].try_into().unwrap()) as usize;

        // Read name bytes starting at 0x40
        let name_bytes = &map_data[0x40..0x40 + name_len];
        let words: Vec<u16> = name_bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let name = String::from_utf16_lossy(&words)
            .trim_end_matches('\0')
            .to_string();
        assert_eq!(name, "AcDb:Classes");
    }

    #[test]
    fn test_build_page_map_format() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        writer.add_section(&mut output, names::HEADER, &vec![0xAA; 100]).unwrap();

        let map_data = writer.build_page_map_ordered(100, 101, 0x400);

        // 2 page map entries + 1 data page entry + null terminator = 4 × 16 = 64 bytes
        assert_eq!(map_data.len(), 64);

        // First entry: page map page (id=100, size=0x400)
        let first_size = i64::from_le_bytes(map_data[0..8].try_into().unwrap());
        let first_id = i64::from_le_bytes(map_data[8..16].try_into().unwrap());
        assert_eq!(first_size, 0x400);
        assert_eq!(first_id, 100);

        // Third entry: the data page (id=1)
        let data_id = i64::from_le_bytes(map_data[40..48].try_into().unwrap());
        assert_eq!(data_id, 1);

        // Null terminator (size=0, id=0)
        let term_size = i64::from_le_bytes(map_data[48..56].try_into().unwrap());
        let term_id = i64::from_le_bytes(map_data[56..64].try_into().unwrap());
        assert_eq!(term_size, 0);
        assert_eq!(term_id, 0);
    }

    // ─── Full write test ────────────────────────────────────────────

    #[test]
    fn test_write_complete_file() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        // Add minimal sections
        writer.add_section(&mut output, names::HEADER, &vec![0xAA; 100]).unwrap();
        writer.add_section(&mut output, names::CLASSES, &vec![0xBB; 50]).unwrap();
        writer.add_section(&mut output, names::SUMMARY_INFO, &vec![0; 10]).unwrap();
        writer.add_section(&mut output, names::PREVIEW, &vec![0xCC; 20]).unwrap();
        writer.add_section(&mut output, names::APP_INFO, &vec![0xDD; 30]).unwrap();
        writer.add_section(&mut output, names::AUX_HEADER, &vec![0; 50]).unwrap();
        writer.add_section(&mut output, names::ACDB_OBJECTS, &vec![0xEE; 300]).unwrap();
        writer.add_section(&mut output, names::HANDLES, &vec![0xFF; 100]).unwrap();

        writer.write_file(&mut output).unwrap();

        let data = output.into_inner();

        // File should start with "AC1021"
        assert_eq!(&data[0..6], b"AC1021");

        // File must be larger than the reserved header
        assert!(data.len() > RESERVED_HEADER_SIZE);

        // File header page at 0x80 should be exactly 0x400 bytes
        // (can verify it's not all zeros)
        assert!(!data[0x80..0x480].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_write_file_metadata_preview_addr() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        writer.add_section(&mut output, names::HEADER, &vec![0xAA; 100]).unwrap();
        writer.add_section(&mut output, names::CLASSES, &vec![0xBB; 50]).unwrap();
        writer.add_section(&mut output, names::PREVIEW, &vec![0xCC; 20]).unwrap();

        writer.write_file(&mut output).unwrap();

        let data = output.into_inner();

        // Preview address at offset 0x0D should be non-zero
        let preview_addr = u32::from_le_bytes(data[0x0D..0x11].try_into().unwrap());
        assert!(preview_addr > 0, "Preview address should be non-zero");
    }

    #[test]
    fn test_file_header_page_has_check_data() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        writer.add_section(&mut output, names::HEADER, &vec![0xAA; 100]).unwrap();

        writer.write_file(&mut output).unwrap();

        let data = output.into_inner();

        // Check data at 0x80 + 0x3D8 = 0x458 should not be all zeros
        let check_data = &data[0x458..0x480];
        assert!(!check_data.iter().all(|&b| b == 0),
            "Check data should be non-zero");
    }

    #[test]
    fn test_file_header_copy_at_end() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = DwgFileHeaderWriterAC21::new(DxfVersion::AC1021, &mut output).unwrap();

        writer.add_section(&mut output, names::HEADER, &vec![0xAA; 100]).unwrap();

        writer.write_file(&mut output).unwrap();

        let data = output.into_inner();

        // The last 0x400 bytes should be a copy of the file header at 0x80
        let file_header = &data[0x80..0x480];
        let header_copy = &data[data.len() - FILE_HEADER_PAGE_SIZE..];
        assert_eq!(file_header, header_copy,
            "File header copy at end should match header at 0x80");
    }

    // ─── RS data page encoding test ─────────────────────────────────

    #[test]
    fn test_rs_encode_data_page_encoding1_size() {
        // encoding=1 → RS(255, 251), block_size=251
        let data = vec![0xAB; 500];
        let encoded = rs_encode_data_page_interleaved(&data, 1);

        // total_size = (500+7)&~7 = 504, factor = ceil(504/251) = 3
        // encoded = 3 × 255 = 765
        assert_eq!(encoded.len(), 3 * 255);
    }

    #[test]
    fn test_rs_encode_data_page_encoding4_size() {
        // encoding=4 also uses RS(255, 251) for data pages per spec §5.4
        let data = vec![0xAB; 500];
        let encoded = rs_encode_data_page_interleaved(&data, 4);

        // total_size = (500+7)&~7 = 504, factor = ceil(504/251) = 3
        // encoded = 3 × 255 = 765
        assert_eq!(encoded.len(), 3 * 255);
    }

    #[test]
    fn test_rs_encode_data_page_single_block() {
        // encoding=1 → RS(255, 251), single block
        let data = vec![0xCD; 100];
        let encoded = rs_encode_data_page_interleaved(&data, 1);

        // total_size = (100+7)&~7 = 104, factor = ceil(104/251) = 1
        // encoded = 1 × 255 = 255
        assert_eq!(encoded.len(), 255);
    }

    #[test]
    fn test_rs_encode_data_page_encoding4_single_block() {
        // encoding=4 also uses RS(255, 251) for data pages per spec §5.4
        let data = vec![0xCD; 100];
        let encoded = rs_encode_data_page_interleaved(&data, 4);

        // total_size = (100+7)&~7 = 104, factor = ceil(104/251) = 1
        // encoded = 1 × 255 = 255
        assert_eq!(encoded.len(), 255);
    }

    #[test]
    fn test_rs_encode_data_page_roundtrip_encoding4() {
        // Verify that encoding → decoding recovers the original data (encoding=4)
        // All data pages use RS(255,251) per spec §5.4 / §5.13
        use crate::io::dwg::reed_solomon::reed_solomon_decode;

        let original = vec![0x42u8; 300];
        let encoded = rs_encode_data_page_interleaved(&original, 4);

        // Reader's decode parameters: all data pages use block_size=251
        let total_size = (300 + 7) & !7; // 304
        let factor = (total_size + 251 - 1) / 251; // ceil(304/251) = 2
        assert_eq!(encoded.len(), factor * 255);

        let mut decoded = vec![0u8; total_size];
        reed_solomon_decode(&encoded, &mut decoded, factor, 251);

        // First 300 bytes should match original
        assert_eq!(&decoded[..300], &original[..]);
    }

    #[test]
    fn test_rs_encode_data_page_roundtrip_encoding1() {
        // Verify that encoding → decoding recovers the original data (encoding=1)
        use crate::io::dwg::reed_solomon::reed_solomon_decode;

        let original = vec![0x77u8; 200];
        let encoded = rs_encode_data_page_interleaved(&original, 1);

        // Reader's decode parameters for encoding=1:
        let total_size = (200 + 7) & !7; // 200 (already aligned)
        let factor = (total_size + 251 - 1) / 251; // ceil(200/251) = 1
        assert_eq!(encoded.len(), factor * 255);

        let mut decoded = vec![0u8; total_size];
        reed_solomon_decode(&encoded, &mut decoded, factor, 251);

        // First 200 bytes should match original
        assert_eq!(&decoded[..200], &original[..]);
    }

    #[test]
    fn test_get_aligned_page_size() {
        assert_eq!(get_aligned_page_size(0x20), 0x20);
        assert_eq!(get_aligned_page_size(0x21), 0x40);
        assert_eq!(get_aligned_page_size(0x3F), 0x40);
        assert_eq!(get_aligned_page_size(0x40), 0x40);
        assert_eq!(get_aligned_page_size(1), 0x20);
    }
}
