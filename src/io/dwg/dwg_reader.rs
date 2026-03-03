//! DWG file reader
//!
//! Reads DWG binary files and extracts structural information including
//! file headers, section metadata, and integrity checksums (CRC values).
//!
//! ## AC1021 (R2007) CRC-64 Extraction
//!
//! The AC1021 format stores a 64-bit CRC in the compressed metadata header.
//! This reader extracts and reports all CRC values, including:
//! - **Header CRC-64**: The master integrity checksum at offset 0x108
//! - **Pages Map CRC**: Checksums for the page directory
//! - **Sections Map CRC**: Checksums for the section directory
//! - **Per-page CRC**: Individual page checksums (in section map entries)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use acadrust::io::dwg::dwg_reader::DwgReader;
//!
//! let reader = DwgReader::from_file("drawing.dwg")?;
//! let info = reader.read_file_header()?;
//!
//! // Access CRC-64 from AC1021 files
//! if let Some(metadata) = &info.ac21_metadata {
//!     println!("Header CRC-64: {:#018X}", metadata.header_crc64);
//! }
//! ```

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Cursor};
use std::fs::File;
use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt};

use crate::error::DxfError;
use crate::notification::{NotificationCollection, NotificationType};
use crate::io::dwg::dwg_version::DwgVersion;
use crate::io::dwg::dwg21_metadata::Dwg21CompressedMetadata;
use crate::io::dwg::reed_solomon::reed_solomon_decode;
use crate::io::dwg::decompressor_ac21::decompress_ac21;

/// AC1021 file header offset (data pages start after this)
const AC21_FILE_HEADER_SIZE: u64 = 0x480;

/// Results from reading a DWG file header.
///
/// Contains version info, section layout, and all extracted CRC values.
#[derive(Debug)]
pub struct DwgFileHeaderInfo {
    /// DWG version string (e.g., "AC1021")
    pub version_string: String,
    /// Parsed DWG version enum
    pub version: DwgVersion,
    /// AutoCAD maintenance version
    pub acad_maintenance_version: u8,
    /// Preview image address
    pub preview_address: i32,
    /// DWG internal version byte
    pub dwg_version: u8,
    /// Application release version
    pub app_release_version: u8,
    /// Drawing code page
    pub code_page: u16,
    /// Security type
    pub security_type: i32,
    /// Summary info address
    pub summary_info_addr: i32,
    /// VBA project address
    pub vba_project_addr: i32,

    // ── AC1021-specific data ──

    /// AC1021 compressed metadata (contains CRC-64 and section layout)
    pub ac21_metadata: Option<Dwg21CompressedMetadata>,
    /// Raw Reed-Solomon decoded values from file header
    pub ac21_header_crc: Option<i64>,
    /// Unknown key from AC1021 header
    pub ac21_unknown_key: Option<i64>,
    /// CRC of compressed data in AC1021 header
    pub ac21_compressed_data_crc: Option<i64>,
    /// Page records: page_id → (offset, size)
    pub page_records: HashMap<i32, (i64, i64)>,
    /// Section descriptors from the section map
    pub section_descriptors: Vec<DwgSectionInfo>,
}

/// Information about a DWG section (from the section map).
#[derive(Debug, Clone)]
pub struct DwgSectionInfo {
    /// Section name (e.g., "AcDb:Header")
    pub name: String,
    /// Compressed size
    pub compressed_size: u64,
    /// Decompressed size
    pub decompressed_size: u64,
    /// Encryption flag
    pub encrypted: u64,
    /// Hash code
    pub hash_code: u64,
    /// Encoding type (4 = Reed-Solomon + LZ77)
    pub encoding: u64,
    /// Number of pages
    pub page_count: u64,
    /// Per-page CRC values and metadata
    pub pages: Vec<DwgPageCrcInfo>,
}

/// CRC information for a single page within a section.
#[derive(Debug, Clone)]
pub struct DwgPageCrcInfo {
    /// Page number
    pub page_number: i64,
    /// Page offset within section
    pub offset: u64,
    /// Page size
    pub size: i64,
    /// Decompressed size
    pub decompressed_size: u64,
    /// Compressed size
    pub compressed_size: u64,
    /// Checksum value
    pub checksum: u64,
    /// **CRC value for this page**
    pub crc: u64,
}

/// DWG file reader with CRC-64 extraction support.
///
/// Reads DWG binary files and provides access to all internal
/// integrity checksums including the AC1021 Header CRC-64.
pub struct DwgReader<R: Read + Seek> {
    stream: R,
    /// Notifications collected during reading
    pub notifications: NotificationCollection,
}

impl DwgReader<File> {
    /// Open a DWG file from a filesystem path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, DxfError> {
        let file = File::open(path)?;
        Ok(Self {
            stream: file,
            notifications: NotificationCollection::new(),
        })
    }
}

impl<R: Read + Seek> DwgReader<R> {
    /// Create a reader from any seekable stream.
    pub fn from_stream(stream: R) -> Self {
        Self {
            stream,
            notifications: NotificationCollection::new(),
        }
    }

    /// Read the DWG file and reconstruct a `CadDocument`.
    ///
    /// This is the main entry point for reading a DWG file into a usable
    /// document. It orchestrates:
    /// 1. File header parsing
    /// 2. Section buffer extraction (Classes, Header, Handles, Objects)
    /// 3. Classes, header variables, and handle map parsing
    /// 4. Object dispatch and entity/object mapping via `DwgDocumentBuilder`
    pub fn read(&mut self) -> std::result::Result<crate::document::CadDocument, DxfError> {
        // 1. Read the DWG file header and section map
        let info = self.read_file_header()?;
        let dxf_version = crate::types::DxfVersion::parse(&info.version_string)
            .unwrap_or(crate::types::DxfVersion::Unknown);
        let mut document = crate::document::CadDocument::with_version(dxf_version);

        // 2. Read Classes (AcDb:Classes)
        if let Ok(classes_buf) = self.get_section_buffer("AcDb:Classes", &info) {
            match crate::io::dwg::dwg_stream_readers::classes_reader::read_classes(
                &classes_buf,
                dxf_version,
            ) {
                Ok(classes) => document.classes = classes,
                Err(e) => self.notifications.notify(
                    NotificationType::Warning,
                    format!("Failed to read classes: {}", e),
                ),
            }
        }

        // 3. Read Header Variables (AcDb:Header)
        if let Ok(header_buf) = self.get_section_buffer("AcDb:Header", &info) {
            match crate::io::dwg::dwg_stream_readers::header_reader::read_header(
                &header_buf,
                dxf_version,
            ) {
                Ok(header_vars) => document.header = header_vars,
                Err(e) => self.notifications.notify(
                    NotificationType::Warning,
                    format!("Failed to read header: {}", e),
                ),
            }
        }

        // 4. Read Handle Map (AcDb:Handles)
        let handle_map = if let Ok(handle_buf) = self.get_section_buffer("AcDb:Handles", &info) {
            match crate::io::dwg::dwg_stream_readers::handle_reader::read_handles(&handle_buf) {
                Ok(hm) => hm,
                Err(e) => {
                    self.notifications.notify(
                        NotificationType::Warning,
                        format!("Failed to read handles: {}", e),
                    );
                    std::collections::HashMap::new()
                }
            }
        } else {
            std::collections::HashMap::new()
        };

        // 5. Read Objects (AcDb:AcDbObjects) and build document
        if !handle_map.is_empty() {
            if let Ok(objects_buf) = self.get_section_buffer("AcDb:AcDbObjects", &info) {
                match crate::io::dwg::dwg_stream_readers::object_reader::DwgObjectReader::new(
                    objects_buf,
                    dxf_version,
                    handle_map,
                ) {
                    Ok(obj_reader) => {
                        let builder = crate::io::dwg::dwg_document_builder::DwgDocumentBuilder::new(obj_reader);
                        if let Err(e) = builder.build(&mut document) {
                            self.notifications.notify(
                                NotificationType::Warning,
                                format!("Object mapping partially failed: {}", e),
                            );
                        }
                    },
                    Err(e) => self.notifications.notify(
                        NotificationType::Warning,
                        format!("Failed to init object reader: {}", e),
                    ),
                }
            }
        }

        Ok(document)
    }

    /// Read the file header and extract all CRC values.
    ///
    /// For AC1021 files, this extracts:
    /// - The Header CRC-64 from the compressed metadata
    /// - Page map CRC values
    /// - Section map CRC values
    /// - Per-page CRC values
    ///
    /// # Returns
    /// A `DwgFileHeaderInfo` containing all extracted data.
    pub fn read_file_header(&mut self) -> Result<DwgFileHeaderInfo, DxfError> {
        self.stream.seek(SeekFrom::Start(0))?;

        // Read version string (6 bytes)
        let mut version_buf = [0u8; 6];
        self.stream.read_exact(&mut version_buf)?;
        let version_string = String::from_utf8_lossy(&version_buf).to_string();

        let version = DwgVersion::from_version_string(&version_string)
            .ok_or_else(|| DxfError::UnsupportedVersion(version_string.clone()))?;

        self.notifications.notify(
            NotificationType::Warning,
            format!("Reading DWG file version: {} ({:?})", version_string, version),
        );

        let mut info = DwgFileHeaderInfo {
            version_string,
            version,
            acad_maintenance_version: 0,
            preview_address: 0,
            dwg_version: 0,
            app_release_version: 0,
            code_page: 0,
            security_type: 0,
            summary_info_addr: 0,
            vba_project_addr: 0,
            ac21_metadata: None,
            ac21_header_crc: None,
            ac21_unknown_key: None,
            ac21_compressed_data_crc: None,
            page_records: HashMap::new(),
            section_descriptors: Vec::new(),
        };

        match version {
            DwgVersion::AC21 => {
                self.read_file_metadata(&mut info)?;
                self.read_file_header_ac21(&mut info)?;
            }
            DwgVersion::AC18 | DwgVersion::AC24 => {
                self.read_file_metadata(&mut info)?;
                self.notifications.notify(
                    NotificationType::Warning,
                    format!("AC18-style header reading: CRC-32 checksums available (not CRC-64). Version: {:?}", version),
                );
            }
            _ => {
                self.notifications.notify(
                    NotificationType::NotSupported,
                    format!("Version {:?} does not use CRC-64 checksums", version),
                );
            }
        }

        Ok(info)
    }

    /// Read common file metadata shared between AC18 and AC21 formats.
    ///
    /// This reads bytes 6–0xFF of the file (after the version string).
    fn read_file_metadata(&mut self, info: &mut DwgFileHeaderInfo) -> Result<(), DxfError> {
        // Skip 5 bytes after version string
        let mut skip = [0u8; 5];
        self.stream.read_exact(&mut skip)?;

        // Maintenance version (1 byte)
        info.acad_maintenance_version = self.stream.read_u8()?;

        // Skip 1 byte
        self.stream.read_exact(&mut [0u8; 1])?;

        // Preview address (4 bytes)
        info.preview_address = self.stream.read_i32::<LittleEndian>()?;

        // DWG version (1 byte)
        info.dwg_version = self.stream.read_u8()?;

        // App release version (1 byte)
        info.app_release_version = self.stream.read_u8()?;

        // Drawing code page (2 bytes)
        info.code_page = self.stream.read_u16::<LittleEndian>()?;

        // Skip 3 bytes
        self.stream.read_exact(&mut [0u8; 3])?;

        // Security type (4 bytes)
        info.security_type = self.stream.read_i32::<LittleEndian>()?;

        // Skip unknown (4 bytes)
        self.stream.read_i32::<LittleEndian>()?;

        // Summary info address (4 bytes)
        info.summary_info_addr = self.stream.read_i32::<LittleEndian>()?;

        // VBA project address (4 bytes)
        info.vba_project_addr = self.stream.read_i32::<LittleEndian>()?;

        // Skip 2 unknown ints (8 bytes)
        self.stream.read_i32::<LittleEndian>()?;
        self.stream.read_i32::<LittleEndian>()?;

        // Skip 80 bytes of padding/unknown data
        let mut pad = [0u8; 80];
        self.stream.read_exact(&mut pad)?;

        Ok(())
    }

    /// Read AC1021 (R2007) file header with CRC-64 extraction.
    ///
    /// This performs the full AC1021 header decoding pipeline:
    /// 1. Reed-Solomon decode the 0x400-byte encoded header
    /// 2. Extract CRC, key, and compression parameters
    /// 3. LZ77 AC21 decompress into 0x110-byte metadata buffer
    /// 4. Parse the `Dwg21CompressedMetadata` (including Header CRC-64)
    /// 5. Decode the page map and section map
    fn read_file_header_ac21(&mut self, info: &mut DwgFileHeaderInfo) -> Result<(), DxfError> {
        // After read_file_metadata, stream is at position 0x80 (128).
        // The Reed-Solomon encoded data follows immediately.
        // Do NOT seek — continue reading from current position.

        // Step 1: Read 0x400 bytes of Reed-Solomon encoded data
        let mut compressed_data = [0u8; 0x400];
        self.stream.read_exact(&mut compressed_data)?;

        // Step 2: Reed-Solomon decode (factor=3, block_size=239)
        let mut decoded_data = vec![0u8; 3 * 239]; // 717 bytes
        reed_solomon_decode(&compressed_data, &mut decoded_data, 3, 239);

        // Step 3: Extract header values from decoded data
        let mut cursor = Cursor::new(&decoded_data);

        let crc = cursor.read_i64::<LittleEndian>()?;
        let unknown_key = cursor.read_i64::<LittleEndian>()?;
        let compressed_data_crc = cursor.read_i64::<LittleEndian>()?;
        let compr_len = cursor.read_i32::<LittleEndian>()?;
        let _length2 = cursor.read_i32::<LittleEndian>()?;

        info.ac21_header_crc = Some(crc);
        info.ac21_unknown_key = Some(unknown_key);
        info.ac21_compressed_data_crc = Some(compressed_data_crc);

        self.notifications.notify(
            NotificationType::Warning,
            format!(
                "AC1021 header: CRC={:#018X}, UnknownKey={:#018X}, CompressedDataCRC={:#018X}, ComprLen={}",
                crc as u64, unknown_key as u64, compressed_data_crc as u64, compr_len
            ),
        );

        // Step 4: Extract 0x110-byte metadata
        let mut metadata_buffer = vec![0u8; 0x110];

        if compr_len < 0 {
            // Negative ComprLen means data is stored uncompressed (raw).
            // |ComprLen| = raw data length. Copy directly from offset 0x20.
            let raw_len = (-compr_len) as usize;
            let src_start = 32; // offset 0x20 in decoded data
            let copy_len = raw_len.min(0x110).min(decoded_data.len().saturating_sub(src_start));
            metadata_buffer[..copy_len].copy_from_slice(&decoded_data[src_start..src_start + copy_len]);
        } else {
            // Positive ComprLen means data is LZ77 compressed.
            // Decompress from byte offset 32 in decoded_data.
            decompress_ac21(&decoded_data, 32, compr_len as u32, &mut metadata_buffer);
        }

        // Step 5: Parse compressed metadata (extracts CRC-64)
        let metadata = Dwg21CompressedMetadata::from_bytes(&metadata_buffer)?;

        self.notifications.notify(
            NotificationType::Warning,
            format!(
                "AC1021 Header CRC-64 extracted: {:#018X}",
                metadata.header_crc64
            ),
        );

        // Note: The exact CRC-64 algorithm used by Autodesk for this field is
        // undocumented. Neither ACadSharp nor any known open reference validates
        // this value. It is stored for informational/round-trip purposes.

        self.notifications.notify(
            NotificationType::Warning,
            format!(
                "AC1021 CRC Seeds: CrcSeed={:#018X}, CrcSeedEncoded={:#018X}, RandomSeed={:#018X}",
                metadata.crc_seed, metadata.crc_seed_encoded, metadata.random_seed
            ),
        );

        self.notifications.notify(
            NotificationType::Warning,
            format!(
                "AC1021 Pages Map CRC: compressed={:#018X}, uncompressed={:#018X}, seed={:#018X}",
                metadata.pages_map_crc_compressed,
                metadata.pages_map_crc_uncompressed,
                metadata.pages_map_crc_seed
            ),
        );

        self.notifications.notify(
            NotificationType::Warning,
            format!(
                "AC1021 Sections Map CRC: compressed={:#018X}, uncompressed={:#018X}, seed={:#018X}",
                metadata.sections_map_crc_compressed,
                metadata.sections_map_crc_uncompressed,
                metadata.sections_map_crc_seed
            ),
        );

        // Step 6: Read page map
        self.read_page_map_ac21(info, &metadata)?;

        // Step 7: Read section map
        self.read_section_map_ac21(info, &metadata)?;

        // Store metadata even if reads above fail (for diagnostics)
        info.ac21_metadata = Some(metadata.clone());

        Ok(())
    }

    /// Read the page map from an AC1021 file.
    ///
    /// The page map lists all data pages and their sizes, allowing
    /// the reader to build a page ID → file offset lookup table.
    fn read_page_map_ac21(
        &mut self,
        info: &mut DwgFileHeaderInfo,
        metadata: &Dwg21CompressedMetadata,
    ) -> Result<(), DxfError> {
        let page_buffer = self.get_page_buffer(
            metadata.pages_map_offset,
            metadata.pages_map_size_compressed,
            metadata.pages_map_size_uncompressed,
            metadata.pages_map_correction_factor,
            0xEF,
        )?;

        let mut cursor = Cursor::new(&page_buffer);
        let mut offset: i64 = 0;

        while (cursor.position() as usize) < page_buffer.len() {
            let size = cursor.read_i64::<LittleEndian>()?;
            let id = cursor.read_i64::<LittleEndian>()?;

            if size == 0 && id == 0 {
                // Terminator — all remaining bytes are padding
                break;
            }

            let ind = id.unsigned_abs();

            info.page_records.insert(ind as i32, (offset, size));
            offset += size;
        }

        self.notifications.notify(
            NotificationType::Warning,
            format!("AC1021: Read {} page records from page map", info.page_records.len()),
        );

        Ok(())
    }

    /// Read the section map from an AC1021 file.
    ///
    /// The section map describes all logical sections (Header, Classes,
    /// Handles, Objects, etc.) and their per-page CRC values.
    fn read_section_map_ac21(
        &mut self,
        info: &mut DwgFileHeaderInfo,
        metadata: &Dwg21CompressedMetadata,
    ) -> Result<(), DxfError> {
        // Look up the section map page
        let sections_map_id = metadata.sections_map_id as i32;
        let seeker = info
            .page_records
            .get(&sections_map_id)
            .map(|&(offset, _)| offset)
            .ok_or_else(|| {
                DxfError::InvalidFormat(format!(
                    "Section map page ID {} not found in page records",
                    sections_map_id
                ))
            })?;

        let section_buffer = self.get_page_buffer_at(
            seeker as u64,
            metadata.sections_map_size_compressed,
            metadata.sections_map_size_uncompressed,
            metadata.sections_map_correction_factor,
            239,
        )?;

        let mut cursor = Cursor::new(&section_buffer);

        while (cursor.position() as usize) < section_buffer.len() {
            // Check if there's enough data for at least the fixed header fields
            if section_buffer.len() - (cursor.position() as usize) < 64 {
                break;
            }

            let compressed_size = cursor.read_u64::<LittleEndian>()?;
            let decompressed_size = cursor.read_u64::<LittleEndian>()?;
            let encrypted = cursor.read_u64::<LittleEndian>()?;
            let hash_code = cursor.read_u64::<LittleEndian>()?;
            let section_name_length = cursor.read_i64::<LittleEndian>()?;
            let _unknown = cursor.read_u64::<LittleEndian>()?;
            let encoding = cursor.read_u64::<LittleEndian>()?;
            let page_count = cursor.read_u64::<LittleEndian>()?;

            // Read section name (UTF-16LE)
            let name = if section_name_length > 0 {
                let byte_len = section_name_length as usize;
                if cursor.position() as usize + byte_len > section_buffer.len() {
                    break;
                }
                let mut name_bytes = vec![0u8; byte_len];
                cursor.read_exact(&mut name_bytes)?;
                // Decode UTF-16LE
                let words: Vec<u16> = name_bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16_lossy(&words)
                    .trim_end_matches('\0')
                    .to_string()
            } else {
                String::new()
            };

            // Read per-page CRC information
            let mut pages = Vec::new();
            for _ in 0..page_count {
                if section_buffer.len() - (cursor.position() as usize) < 56 {
                    break;
                }
                let page_offset = cursor.read_u64::<LittleEndian>()?;
                let page_size = cursor.read_i64::<LittleEndian>()?;
                let page_number = cursor.read_i64::<LittleEndian>()?;
                let page_decompressed_size = cursor.read_u64::<LittleEndian>()?;
                let page_compressed_size = cursor.read_u64::<LittleEndian>()?;
                let page_checksum = cursor.read_u64::<LittleEndian>()?;
                let page_crc = cursor.read_u64::<LittleEndian>()?;

                self.notifications.notify(
                    NotificationType::Warning,
                    format!(
                        "  Section '{}' page {}: CRC={:#018X}, Checksum={:#018X}, CompSize={}, DecompSize={}",
                        name, page_number, page_crc, page_checksum,
                        page_compressed_size, page_decompressed_size
                    ),
                );

                pages.push(DwgPageCrcInfo {
                    page_number,
                    offset: page_offset,
                    size: page_size,
                    decompressed_size: page_decompressed_size,
                    compressed_size: page_compressed_size,
                    checksum: page_checksum,
                    crc: page_crc,
                });
            }

            if section_name_length > 0 {
                info.section_descriptors.push(DwgSectionInfo {
                    name: name.clone(),
                    compressed_size,
                    decompressed_size,
                    encrypted,
                    hash_code,
                    encoding,
                    page_count,
                    pages,
                });
            }
        }

        self.notifications.notify(
            NotificationType::Warning,
            format!(
                "AC1021: Read {} section descriptors from section map",
                info.section_descriptors.len()
            ),
        );

        Ok(())
    }

    /// Get a decompressed page buffer from the file.
    ///
    /// Handles the Reed-Solomon + LZ77 AC21 decompression pipeline.
    fn get_page_buffer(
        &mut self,
        page_offset: u64,
        compressed_size: u64,
        uncompressed_size: u64,
        correction_factor: u64,
        block_size: usize,
    ) -> Result<Vec<u8>, DxfError> {
        self.get_page_buffer_at(
            page_offset,
            compressed_size,
            uncompressed_size,
            correction_factor,
            block_size,
        )
    }

    /// Get a decompressed page buffer from a specific file offset.
    fn get_page_buffer_at(
        &mut self,
        page_offset: u64,
        compressed_size: u64,
        uncompressed_size: u64,
        correction_factor: u64,
        block_size: usize,
    ) -> Result<Vec<u8>, DxfError> {
        // Calculate sizes matching ACadSharp's getPageBuffer()
        let v = compressed_size.wrapping_add(7);
        let v1 = v & 0xFFFF_FFF8; // Align to 8 bytes

        let total_size = v1.wrapping_mul(correction_factor) as usize;

        if total_size == 0 || total_size > 100_000_000 {
            return Err(DxfError::InvalidFormat(format!(
                "Invalid page buffer size: {} (compressed={}, factor={}, v1={})",
                total_size, compressed_size, correction_factor, v1
            )));
        }

        let factor = (total_size + block_size - 1) / block_size;
        let read_length = factor * 255;

        // Read encoded data from file
        self.stream.seek(SeekFrom::Start(AC21_FILE_HEADER_SIZE + page_offset))?;
        let mut encoded_buffer = vec![0u8; read_length];
        let bytes_read = self.stream.read(&mut encoded_buffer)?;
        if bytes_read < read_length {
            // Pad remaining with zeros
            encoded_buffer[bytes_read..].fill(0);
        }

        // Reed-Solomon decode
        let mut compressed_data = vec![0u8; total_size];
        reed_solomon_decode(&encoded_buffer, &mut compressed_data, factor, block_size);

        // LZ77 AC21 decompress.
        // Some writers store compressed data even when the compressed size is
        // not smaller than the uncompressed size (the ODA spec suggests data
        // should be stored raw in that case, but not all implementations follow
        // this convention).  Always decompress when the sizes differ; only skip
        // when they are exactly equal (meaning data was stored raw).
        if compressed_size != uncompressed_size {
            // AC21 decompressor may read/write slightly past declared sizes
            // due to block-level copy operations (4/8/32 byte chunks).
            // Pad both source and destination buffers.
            let src_padded_size = compressed_data.len() + 64;
            let mut padded_source = vec![0u8; src_padded_size];
            padded_source[..compressed_data.len()].copy_from_slice(&compressed_data);

            let dst_padded_size = uncompressed_size as usize + 64;
            let mut decompressed_data = vec![0u8; dst_padded_size];
            decompress_ac21(
                &padded_source,
                0,
                compressed_size as u32,
                &mut decompressed_data,
            );
            decompressed_data.truncate(uncompressed_size as usize);
            Ok(decompressed_data)
        } else {
            // compressed_size == uncompressed_size: data is stored raw
            compressed_data.truncate(uncompressed_size as usize);
            Ok(compressed_data)
        }
    }

    /// Get the merged decompressed buffer for a named section (AC21).
    ///
    /// Reads and concatenates all pages belonging to the given section,
    /// producing the complete section data ready for parsing.
    ///
    /// # Arguments
    /// * `section_name` - Section name (e.g., "AcDb:Header", "AcDb:Classes")
    /// * `info` - Previously read file header info containing section descriptors
    ///
    /// # Returns
    /// The complete decompressed section buffer, or an error if the section
    /// is not found or a page cannot be read.
    pub fn get_section_buffer(
        &mut self,
        section_name: &str,
        info: &DwgFileHeaderInfo,
    ) -> Result<Vec<u8>, DxfError> {
        // Find the section descriptor
        let section = info.section_descriptors.iter()
            .find(|s| s.name == section_name)
            .ok_or_else(|| DxfError::Parse(
                format!("Section '{}' not found in file", section_name)
            ))?;

        // Field 0x00 ("Data size") holds the total section data size.
        // Field 0x08 ("Max size") is the page partition size per spec §5.4.
        // We truncate to the total data size, not the page size.
        let total_size = section.compressed_size as usize;
        let mut result = Vec::with_capacity(total_size);

        // encoding=1 (stored): data is stored raw — no RS encoding, no LZ77.
        // encoding=4 (compressed): data is LZ77-compressed then RS-encoded with RS(255,251).
        // System pages (page map, section map) use RS(255,239) per §5.3,
        // but those are decoded separately in read_page_map_ac21 / read_section_map_ac21.
        let encoding = section.encoding;
        let block_size: usize = 251;

        for page in &section.pages {
            // Look up the page record to get the file offset
            if let Some(&(page_offset, _page_size)) = info.page_records.get(&(page.page_number as i32)) {
                let page_data = if encoding == 1 {
                    // encoding=1: read raw data directly (no RS, no LZ77).
                    // AutoCAD stores encoding=1 pages as raw bytes aligned to 32.
                    let read_size = page.decompressed_size as usize;
                    self.stream.seek(SeekFrom::Start(AC21_FILE_HEADER_SIZE + page_offset as u64))?;
                    let mut buf = vec![0u8; read_size];
                    self.stream.read_exact(&mut buf)?;
                    buf
                } else {
                    self.get_page_buffer_at(
                        page_offset as u64,
                        page.compressed_size,
                        page.decompressed_size,
                        1, // correction factor is always 1 for data pages
                        block_size,
                    )?
                };

                result.extend_from_slice(&page_data);
            } else {
                return Err(DxfError::Parse(
                    format!("Page {} not found in page map", page.page_number)
                ));
            }
        }

        // Truncate to the declared section size (last page may be padded)
        result.truncate(total_size);

        Ok(result)
    }

    /// Find a section descriptor by name.
    pub fn find_section<'a>(
        info: &'a DwgFileHeaderInfo,
        name: &str,
    ) -> Option<&'a DwgSectionInfo> {
        info.section_descriptors.iter().find(|s| s.name == name)
    }

    /// Extract all CRC values from the file and return them as a summary.
    ///
    /// This is a convenience method that reads the file header and
    /// formats all CRC values for display.
    pub fn extract_all_crcs(&mut self) -> Result<CrcExtractionReport, DxfError> {
        let info = self.read_file_header()?;

        let mut report = CrcExtractionReport {
            version: info.version_string.clone(),
            header_crc64: None,
            header_crc: info.ac21_header_crc,
            compressed_data_crc: info.ac21_compressed_data_crc,
            pages_map_crc_compressed: None,
            pages_map_crc_uncompressed: None,
            pages_map_crc_seed: None,
            sections_map_crc_compressed: None,
            sections_map_crc_uncompressed: None,
            sections_map_crc_seed: None,
            crc_seed: None,
            crc_seed_encoded: None,
            random_seed: None,
            page_crcs: Vec::new(),
            notifications: std::mem::take(&mut self.notifications),
        };

        if let Some(ref metadata) = info.ac21_metadata {
            report.header_crc64 = Some(metadata.header_crc64);
            report.pages_map_crc_compressed = Some(metadata.pages_map_crc_compressed);
            report.pages_map_crc_uncompressed = Some(metadata.pages_map_crc_uncompressed);
            report.pages_map_crc_seed = Some(metadata.pages_map_crc_seed);
            report.sections_map_crc_compressed = Some(metadata.sections_map_crc_compressed);
            report.sections_map_crc_uncompressed = Some(metadata.sections_map_crc_uncompressed);
            report.sections_map_crc_seed = Some(metadata.sections_map_crc_seed);
            report.crc_seed = Some(metadata.crc_seed);
            report.crc_seed_encoded = Some(metadata.crc_seed_encoded);
            report.random_seed = Some(metadata.random_seed);
        }

        for section in &info.section_descriptors {
            for page in &section.pages {
                report.page_crcs.push(PageCrcEntry {
                    section_name: section.name.clone(),
                    page_number: page.page_number,
                    crc: page.crc,
                    checksum: page.checksum,
                    compressed_size: page.compressed_size,
                    decompressed_size: page.decompressed_size,
                });
            }
        }

        Ok(report)
    }
}

/// Complete CRC extraction report from a DWG file.
#[derive(Debug)]
pub struct CrcExtractionReport {
    /// DWG version string
    pub version: String,
    /// Header CRC-64 (AC1021 only)
    pub header_crc64: Option<u64>,
    /// Header CRC from Reed-Solomon decoded data
    pub header_crc: Option<i64>,
    /// Compressed data CRC
    pub compressed_data_crc: Option<i64>,
    /// Pages map CRC (compressed)
    pub pages_map_crc_compressed: Option<u64>,
    /// Pages map CRC (uncompressed)
    pub pages_map_crc_uncompressed: Option<u64>,
    /// Pages map CRC seed
    pub pages_map_crc_seed: Option<u64>,
    /// Sections map CRC (compressed)
    pub sections_map_crc_compressed: Option<u64>,
    /// Sections map CRC (uncompressed)
    pub sections_map_crc_uncompressed: Option<u64>,
    /// Sections map CRC seed
    pub sections_map_crc_seed: Option<u64>,
    /// Global CRC seed
    pub crc_seed: Option<u64>,
    /// Encoded CRC seed
    pub crc_seed_encoded: Option<u64>,
    /// Random seed
    pub random_seed: Option<u64>,
    /// Per-page CRC values
    pub page_crcs: Vec<PageCrcEntry>,
    /// Notifications collected during reading
    pub notifications: NotificationCollection,
}

/// CRC entry for a single page.
#[derive(Debug, Clone)]
pub struct PageCrcEntry {
    /// Section name
    pub section_name: String,
    /// Page number
    pub page_number: i64,
    /// CRC value
    pub crc: u64,
    /// Checksum value
    pub checksum: u64,
    /// Compressed size
    pub compressed_size: u64,
    /// Decompressed size
    pub decompressed_size: u64,
}

impl std::fmt::Display for CrcExtractionReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== DWG CRC Extraction Report ===")?;
        writeln!(f, "Version: {}", self.version)?;
        writeln!(f)?;

        if let Some(crc64) = self.header_crc64 {
            writeln!(f, "--- Header CRC-64 ---")?;
            writeln!(f, "  Header CRC-64:              {:#018X}", crc64)?;
        }

        if let Some(crc) = self.header_crc {
            writeln!(f, "  Header CRC (RS decoded):    {:#018X}", crc as u64)?;
        }
        if let Some(crc) = self.compressed_data_crc {
            writeln!(f, "  Compressed Data CRC:        {:#018X}", crc as u64)?;
        }

        if self.crc_seed.is_some() {
            writeln!(f)?;
            writeln!(f, "--- CRC Seeds ---")?;
            if let Some(v) = self.crc_seed {
                writeln!(f, "  CRC Seed:                   {:#018X}", v)?;
            }
            if let Some(v) = self.crc_seed_encoded {
                writeln!(f, "  CRC Seed Encoded:           {:#018X}", v)?;
            }
            if let Some(v) = self.random_seed {
                writeln!(f, "  Random Seed:                {:#018X}", v)?;
            }
        }

        if self.pages_map_crc_compressed.is_some() {
            writeln!(f)?;
            writeln!(f, "--- Pages Map CRC ---")?;
            if let Some(v) = self.pages_map_crc_compressed {
                writeln!(f, "  Compressed:                 {:#018X}", v)?;
            }
            if let Some(v) = self.pages_map_crc_uncompressed {
                writeln!(f, "  Uncompressed:               {:#018X}", v)?;
            }
            if let Some(v) = self.pages_map_crc_seed {
                writeln!(f, "  Seed:                       {:#018X}", v)?;
            }
        }

        if self.sections_map_crc_compressed.is_some() {
            writeln!(f)?;
            writeln!(f, "--- Sections Map CRC ---")?;
            if let Some(v) = self.sections_map_crc_compressed {
                writeln!(f, "  Compressed:                 {:#018X}", v)?;
            }
            if let Some(v) = self.sections_map_crc_uncompressed {
                writeln!(f, "  Uncompressed:               {:#018X}", v)?;
            }
            if let Some(v) = self.sections_map_crc_seed {
                writeln!(f, "  Seed:                       {:#018X}", v)?;
            }
        }

        if !self.page_crcs.is_empty() {
            writeln!(f)?;
            writeln!(f, "--- Per-Page CRC Values ---")?;
            for entry in &self.page_crcs {
                writeln!(
                    f,
                    "  {} [page {}]: CRC={:#018X}, Checksum={:#018X} (comp={}, decomp={})",
                    entry.section_name,
                    entry.page_number,
                    entry.crc,
                    entry.checksum,
                    entry.compressed_size,
                    entry.decompressed_size
                )?;
            }
        }

        Ok(())
    }
}
