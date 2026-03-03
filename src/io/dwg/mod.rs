//! DWG binary file format support
//!
//! This module provides support for reading and writing DWG binary files,
//! AutoCAD's native file format. DWG files use bit-granularity encoding,
//! version-specific data layouts, and LZ77 compression (R2004+).
//!
//! ## Supported versions
//!
//! | DWG Version | AutoCAD | File format  |
//! |-------------|---------|-------------|
//! | AC1012      | R13     | Linear      |
//! | AC1014      | R14     | Linear      |
//! | AC1015      | R2000   | Linear      |
//! | AC1018      | R2004   | Paged + LZ77 |
//! | AC1021      | R2007   | Paged + LZ77 (read only) |
//! | AC1024      | R2010   | Paged + LZ77 |
//! | AC1027      | R2013   | Paged + LZ77 |
//! | AC1032      | R2018   | Paged + LZ77 |

pub mod checksum;
pub mod compression;
pub mod compressor_ac21;
pub mod crc;
pub mod decompressor_ac21;
pub mod dwg21_metadata;
pub mod dwg_document_builder;
pub mod dwg_reader;
pub mod dwg_reference_type;
pub mod dwg_stream_readers;
pub mod dwg_stream_writers;
pub mod dwg_version;
pub mod dwg_writer;
pub mod file_headers;
pub mod reed_solomon;

pub use dwg_reader::DwgReader;
pub use dwg_reference_type::DwgReferenceType;
pub use dwg_version::DwgVersion;
pub use dwg_writer::DwgWriter;
pub use file_headers::{DwgFileHeaderWriterAC15, DwgFileHeaderWriterAC18};
