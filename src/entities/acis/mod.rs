//! ACIS/SAT solid modeler data parser and writer.
//!
//! This module provides support for parsing and generating ACIS SAT (Save And
//! Restore) format data, which is the text-based geometry representation used
//! by 3DSOLID, BODY, and REGION entities in DXF/DWG files.
//!
//! # Supported Versions
//!
//! - **ACIS 1.5–6.0** (SAT versions 400–600): Legacy format without explicit
//!   record indices.
//! - **ACIS 7.0** (SAT version 700): Adds explicit negative record indices,
//!   `asmheader` entity, and `@`-prefixed counted strings.
//! - **ACIS 21.0+** (SAT version 21800): Modern format with extended entity types.
//!
//! # Format Overview
//!
//! A SAT file consists of:
//! 1. A **header** (3–4 lines): version, product info, tolerances
//! 2. **Entity records**: each terminated by `#`, representing the B-rep
//!    topology (body → lump → shell → face → loop → coedge → edge → vertex)
//!    and associated geometry (surfaces, curves, points, transforms).
//!
//! # Example
//!
//! ```rust
//! use acadrust::entities::acis::{SatDocument, SatVersion};
//!
//! // Parse SAT text from a 3DSOLID entity
//! let sat_text = "700 0 1 0\n\
//!     @12 Spatial Corp @7 ACIS 7.0 @24 Thu Jan 01 00:00:00 2023\n\
//!     1e-06 9.9999999999999995e-07\n\
//!     -0 asmheader $-1 -1 @12 700 7 0 0 @5 ACIS @3 7.0 @24 Thu Jan 01 00:00:00 2023 #\n\
//!     -1 body $-1 $-1 $-1 $-1 #\n\
//!     End-of-ACIS-data\n";
//!
//! let doc = SatDocument::parse(sat_text).unwrap();
//! assert_eq!(doc.header.version, SatVersion::new(7, 0, 0));
//! assert_eq!(doc.records.len(), 2);
//!
//! // Write back to SAT text
//! let output = doc.to_sat_string();
//! assert!(output.contains("700"));
//! ```

pub mod types;
pub mod parser;
pub mod writer;

pub use types::*;
pub use parser::SatParser;
pub use writer::SatWriter;
