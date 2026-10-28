//! Non-graphical Arch object.
//!
//! Styles, display representations, anchors, list definitions, and the other
//! AEC dictionary objects keep the original DWG body. The class name is the
//! supported identity; the bytes are what Arch reads back.

use crate::types::{DxfVersion, Handle};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ArchObject {
    pub handle: Handle,
    pub owner: Handle,
    pub reactors: Vec<Handle>,
    pub xdictionary_handle: Option<Handle>,
    pub dxf_name: String,
    pub cpp_class_name: String,
    pub raw_dxf_codes: Option<Vec<(i32, String)>>,
    pub raw_dwg_data: Option<Vec<u8>>,
    pub raw_dwg_handle_bits: i64,
    pub raw_dwg_version: Option<DxfVersion>,
}

impl ArchObject {
    pub fn new(dxf_name: impl Into<String>, handle: Handle, owner: Handle) -> Self {
        let dxf_name = dxf_name.into();
        let cpp_class_name = crate::classes::arch::cpp_class_name(&dxf_name)
            .unwrap_or("")
            .to_string();
        Self {
            handle,
            owner,
            reactors: Vec::new(),
            xdictionary_handle: None,
            dxf_name,
            cpp_class_name,
            raw_dxf_codes: None,
            raw_dwg_data: None,
            raw_dwg_handle_bits: 0,
            raw_dwg_version: None,
        }
    }
}
