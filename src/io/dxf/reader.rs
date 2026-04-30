//! DXF file reader

mod stream_reader;
mod text_reader;
mod binary_reader;
mod section_reader;

pub use stream_reader::DxfStreamReader;
pub use text_reader::DxfTextReader;
pub use binary_reader::DxfBinaryReader;

use section_reader::SectionReader;

use crate::document::CadDocument;
use crate::error::Result;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

/// Configuration for the DXF reader.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DxfReaderConfiguration {
    /// When `true`, parse errors within individual entities/objects/sections
    /// are caught and reported as notifications instead of aborting the read.
    ///
    /// Default: `false` (strict mode — errors propagate).
    pub failsafe: bool,

    /// Default encoding to use for non-UTF8 strings if the DXF file does not
    /// specify it via $DWGCODEPAGE.
    ///
    /// Only applies to DXF versions prior to AC1021 (AutoCAD 2007).
    pub default_encoding: Option<String>,
}

impl Default for DxfReaderConfiguration {
    fn default() -> Self {
        Self {
            failsafe: false,
            default_encoding: None,
        }
    }
}

/// DXF file reader
pub struct DxfReader {
    reader: Box<dyn DxfStreamReader>,
    config: DxfReaderConfiguration,
    /// Estimated entity count based on stream size (used for pre-allocation).
    estimated_entities: usize,
}

impl DxfReader {
    /// Create a new DXF reader from any reader
    pub fn from_reader<R: Read + Seek + 'static>(reader: R) -> Result<Self> {
        let mut buf_reader = BufReader::with_capacity(64 * 1024, reader);

        // Estimate entity count from stream size (~300 bytes per entity on average)
        let stream_size = buf_reader.seek(std::io::SeekFrom::End(0)).unwrap_or(0);
        buf_reader.seek(std::io::SeekFrom::Start(0))?;
        let estimated_entities = (stream_size as usize / 300).max(16);

        // Detect if binary
        let is_binary = Self::is_binary(&mut buf_reader)?;

        // Create appropriate reader
        let reader: Box<dyn DxfStreamReader> = if is_binary {
            Box::new(DxfBinaryReader::new(buf_reader)?)
        } else {
            // Seek back to start for text DXF files
            buf_reader.seek(std::io::SeekFrom::Start(0))?;
            Box::new(DxfTextReader::new(buf_reader)?)
        };

        Ok(Self {
            reader,
            config: DxfReaderConfiguration::default(),
            estimated_entities,
        })
    }

    /// Create a new DXF reader from a file path
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let mut buf_reader = BufReader::with_capacity(64 * 1024, file);

        // Estimate entity count from stream size (~300 bytes per entity on average)
        let stream_size = buf_reader.seek(std::io::SeekFrom::End(0)).unwrap_or(0);
        buf_reader.seek(std::io::SeekFrom::Start(0))?;
        let estimated_entities = (stream_size as usize / 300).max(16);
        
        // Detect if binary
        let is_binary = Self::is_binary(&mut buf_reader)?;
        
        // Create appropriate reader
        let reader: Box<dyn DxfStreamReader> = if is_binary {
            Box::new(DxfBinaryReader::new(buf_reader)?)
        } else {
            // Seek back to start for text DXF files
            buf_reader.seek(std::io::SeekFrom::Start(0))?;
            Box::new(DxfTextReader::new(buf_reader)?)
        };
        
        Ok(Self {
            reader,
            config: DxfReaderConfiguration::default(),
            estimated_entities,
        })
    }
    
    /// Check if a stream contains binary DXF data
    fn is_binary<R: Read + Seek>(reader: &mut R) -> Result<bool> {
        const SENTINEL: &[u8] = b"AutoCAD Binary DXF";
        let mut buffer = vec![0u8; SENTINEL.len()];
        
        // Try to read the sentinel bytes
        let bytes_read = reader.read(&mut buffer)?;
        
        // Always seek back to start after checking
        reader.seek(std::io::SeekFrom::Start(0))?;
        
        // If file is too small or doesn't match, it's not binary
        if bytes_read < SENTINEL.len() {
            return Ok(false);
        }
        
        Ok(buffer == SENTINEL)
    }

    /// Set the reader configuration.
    pub fn with_configuration(mut self, config: DxfReaderConfiguration) -> Self {
        self.config = config;
        self
    }

    /// Read a DXF file and return a CadDocument
    pub fn read(mut self) -> Result<CadDocument> {
        // Set default encoding if provided
        if let Some(ref encoding_name) = self.config.default_encoding {
            if let Some(enc) = crate::io::dxf::code_page::encoding_from_code_page(encoding_name) {
                self.reader.set_encoding(enc);
            }
        }

        // Create document with pre-allocated entity storage
        let mut document = CadDocument::new();
        document.entities.reserve(self.estimated_entities);
        document.entity_index.reserve(self.estimated_entities);
        
        // Read all sections
        let failsafe = self.config.failsafe;

        while let Some(pair) = self.reader.read_pair()? {
            if pair.code == 0 && pair.value_string == "SECTION" {
                // Read section name
                if let Some(section_pair) = self.reader.read_pair()? {
                    if section_pair.code == 2 {
                        let section_name = section_pair.value_string.clone();
                        let result = match section_name.as_str() {
                            "HEADER" => self.read_header_section(&mut document),
                            "CLASSES" => self.read_classes_section(&mut document),
                            "TABLES" => self.read_tables_section(&mut document),
                            "BLOCKS" => self.read_blocks_section(&mut document),
                            "ENTITIES" => self.read_entities_section(&mut document),
                            "OBJECTS" => self.read_objects_section(&mut document),
                            "THUMBNAILIMAGE" => {
                                document.notifications.notify(
                                    crate::notification::NotificationType::NotImplemented,
                                    "THUMBNAILIMAGE section skipped",
                                );
                                self.skip_section()
                            }
                            _ => {
                                // Skip unknown section
                                self.skip_section()
                            }
                        };

                        // In failsafe mode, catch errors and continue
                        if let Err(e) = result {
                            if failsafe {
                                document.notifications.notify(
                                    crate::notification::NotificationType::Error,
                                    format!("Error reading {} section: {}", section_name, e),
                                );
                                // Try to skip to the end of the section
                                let _ = self.skip_section();
                            } else {
                                return Err(e);
                            }
                        }
                    }
                }
            } else if pair.code == 0 && pair.value_string == "EOF" {
                break;
            }
        }

        // Post-read resolution: assign owner handles and update next_handle
        document.resolve_references();

        Ok(document)
    }
    
    /// Read the HEADER section
    fn read_header_section(&mut self, document: &mut CadDocument) -> Result<()> {
        let mut section_reader = SectionReader::new(&mut self.reader);
        section_reader.read_header(document)
    }

    /// Read the CLASSES section
    fn read_classes_section(&mut self, document: &mut CadDocument) -> Result<()> {
        let mut section_reader = SectionReader::new(&mut self.reader);
        section_reader.read_classes(document)
    }

    /// Read the TABLES section
    fn read_tables_section(&mut self, document: &mut CadDocument) -> Result<()> {
        let mut section_reader = SectionReader::new(&mut self.reader);
        section_reader.read_tables(document)
    }

    /// Read the BLOCKS section
    fn read_blocks_section(&mut self, document: &mut CadDocument) -> Result<()> {
        let mut section_reader = SectionReader::new(&mut self.reader);
        section_reader.read_blocks(document)
    }

    /// Read the ENTITIES section
    fn read_entities_section(&mut self, document: &mut CadDocument) -> Result<()> {
        let mut section_reader = SectionReader::new(&mut self.reader);
        section_reader.read_entities(document)
    }

    /// Read the OBJECTS section
    fn read_objects_section(&mut self, document: &mut CadDocument) -> Result<()> {
        let mut section_reader = SectionReader::new(&mut self.reader);
        section_reader.read_objects(document)
    }
    
    /// Skip the current section
    fn skip_section(&mut self) -> Result<()> {
        while let Some(pair) = self.reader.read_pair()? {
            if pair.code == 0 && pair.value_string == "ENDSEC" {
                break;
            }
        }
        Ok(())
    }
}
