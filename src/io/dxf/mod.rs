//! DXF (Drawing Exchange Format) reading and writing.
//!
//! Supports both **ASCII** and **Binary** DXF for versions R12 (AC1009)
//! through R2018+ (AC1032).
//!
//! # Reading
//!
//! ```rust,ignore
//! use acadrust::DxfReader;
//!
//! let doc = DxfReader::from_file("drawing.dxf")?.read()?;
//! ```
//!
//! # Writing
//!
//! ```rust,ignore
//! use acadrust::DxfWriter;
//!
//! DxfWriter::new(&doc).write_to_file("output.dxf")?;
//! ```

mod dxf_code;
mod group_code_value;
mod reader;
mod writer;
pub mod code_page;

pub use dxf_code::DxfCode;
pub use group_code_value::GroupCodeValueType;
pub use reader::{DxfReader, DxfReaderConfiguration};
pub use writer::{DxfWriter, DxfStreamWriter, DxfStreamWriterExt, DxfTextWriter, DxfBinaryWriter, SectionWriter};
pub use writer::{write_dxf, write_binary_dxf, value_type_for_code};


