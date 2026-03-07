//! File I/O for DXF and DWG formats.
//!
//! | Sub-module | Capabilities |
//! |------------|----------------------------------------------------|
//! | [`dxf`]    | Read/write ASCII and Binary DXF (R12 – R2018+) |
//! | [`dwg`]    | Read/write native DWG binary (R13 – R2018) |
//!
//! The top-level re-exports [`DxfReader`], [`DxfWriter`], [`DwgReader`],
//! and [`DwgWriter`] for quick access.

pub mod dxf;
pub mod dwg;

pub use dxf::{DxfReader, DxfWriter};
pub use dwg::{DwgReader, DwgWriter};

