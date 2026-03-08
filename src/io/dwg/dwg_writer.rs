//! Top-level DWG file writer
//!
//! Orchestrates all section writers to produce a complete DWG binary file.
//! Supports three file formats:
//!
//! - **AC15** (R13/R14/R2000): Linear format with sequential sections
//! - **AC18** (R2004/R2010+): Page-based format with LZ77 compression
//! - **AC21** (R2007): RS-encoded pages with LZ77 AC21 compression and CRC-64
//!
//! ## Usage
//!
//! ```no_run
//! use acadrust::document::CadDocument;
//! use acadrust::io::dwg::DwgWriter;
//!
//! let doc = CadDocument::new();
//! DwgWriter::write_to_file("output.dwg", &doc).unwrap();
//! ```
//!
//! Based on ACadSharp's `DwgWriter` class.

use std::fs::File;
use std::io::{BufWriter, Cursor, Seek, Write};
use std::path::Path;

use crate::document::{CadDocument, HeaderVariables};
use crate::error::{DxfError, Result};
use crate::types::DxfVersion;

use super::dwg_stream_writers::{
    app_info_writer, aux_header_writer, classes_writer, handle_writer, header_writer,
    preview_writer, DwgObjectWriter,
};
use super::file_headers::{
    section_names, DwgFileHeaderWriterAC15, DwgFileHeaderWriterAC18, DwgFileHeaderWriterAC21,
};

// ════════════════════════════════════════════════════════════════════════════
//  Public API
// ════════════════════════════════════════════════════════════════════════════

/// DWG binary file writer.
///
/// Produces a complete DWG file from a [`CadDocument`].
/// The output version is determined by [`CadDocument::version`].
pub struct DwgWriter;

impl DwgWriter {
    /// Write a DWG file to the given path.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The version is `Unknown`
    /// - An I/O error occurs
    /// - The document contains invalid data
    pub fn write_to_file<P: AsRef<Path>>(path: P, document: &CadDocument) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        Self::write_to_writer(writer, document)
    }

    /// Write a DWG file to any `Write + Seek` output.
    pub fn write_to_writer<W: Write + Seek>(mut output: W, document: &CadDocument) -> Result<()> {
        validate_version(document.version)?;
        let version = document.version;

        if uses_ac21_format(version) {
            write_ac21(&mut output, document, version)
        } else if uses_paged_format(version) {
            write_ac18(&mut output, document, version)
        } else {
            write_ac15(&mut output, document, version)
        }
    }

    /// Write an AC21 DWG file **without LZ77 compression** (diagnostic).
    ///
    /// Pages are still RS-encoded but LZ77 is bypassed, storing raw data.
    /// Useful for isolating whether a read error is caused by compression
    /// or by the object data itself.
    pub fn write_to_file_no_lz77<P: AsRef<Path>>(path: P, document: &CadDocument) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        Self::write_to_writer_no_lz77(writer, document)
    }

    /// Write an AC21 DWG without LZ77 to any `Write + Seek` output.
    pub fn write_to_writer_no_lz77<W: Write + Seek>(
        mut output: W,
        document: &CadDocument,
    ) -> Result<()> {
        validate_version(document.version)?;
        write_ac21_impl(&mut output, document, document.version, true)
    }

    /// Write a DWG file to a byte vector (useful for testing).
    pub fn write_to_vec(document: &CadDocument) -> Result<Vec<u8>> {
        let mut buffer = Cursor::new(Vec::new());
        Self::write_to_writer(&mut buffer, document)?;
        Ok(buffer.into_inner())
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Validation
// ════════════════════════════════════════════════════════════════════════════

/// Validate that the document version is supported for DWG writing.
fn validate_version(version: DxfVersion) -> Result<()> {
    match version {
        DxfVersion::Unknown => Err(DxfError::UnsupportedVersion(
            "Unknown version".to_string(),
        )),
        _ => Ok(()),
    }
}

/// Whether the version uses the AC21 (R2007) file format.
///
/// AC1021 uses RS-encoded pages, LZ77 AC21 compression, and CRC-64
/// checksums — distinct from both the AC18 and AC15 formats.
fn uses_ac21_format(version: DxfVersion) -> bool {
    version == DxfVersion::AC1021
}

/// Whether the version uses the AC18 page-based file format.
fn uses_paged_format(version: DxfVersion) -> bool {
    version >= DxfVersion::AC1018
}

/// Prepare the header for writing by synchronizing all handle references
/// from the actual document objects and correcting the handle seed.
///
/// This is critical because after a DWG read-roundtrip, header handle
/// references may be NULL (the header reader for R2007+ doesn't correctly
/// read handles from the three-stream merged format). Without syncing,
/// the header would write NULL handles for table controls and the root
/// dictionary, causing IntelliCAD (and other CAD apps) to report
/// "null object id" for every object.
///
/// Also updates EXTMIN/EXTMAX from the computed model-space extents so that
/// "Zoom Extents" works correctly when the file is first opened.
fn prepare_header(
    document: &CadDocument,
    handle_map: &[(u64, u32)],
    extents: &Option<crate::types::BoundingBox3D>,
) -> HeaderVariables {
    let mut h = document.header.clone();

    // ── Sync table control handles from actual table objects ──
    // The tables always have valid handles from initialize_defaults(),
    // but the header might have NULL handles after a DWG read.
    h.block_control_handle = document.block_records.handle();
    h.layer_control_handle = document.layers.handle();
    h.style_control_handle = document.text_styles.handle();
    h.linetype_control_handle = document.line_types.handle();
    h.view_control_handle = document.views.handle();
    h.ucs_control_handle = document.ucss.handle();
    h.vport_control_handle = document.vports.handle();
    h.appid_control_handle = document.app_ids.handle();
    h.dimstyle_control_handle = document.dim_styles.handle();

    // ── Sync root dictionary handle ──
    // Find the root dictionary by scanning document.objects for a
    // Dictionary with owner == NULL. Prefer non-0x0C handles (file's
    // root dict) over the initialize_defaults() one.
    if h.named_objects_dict_handle.is_null() {
        h.named_objects_dict_handle = find_root_dict_handle(&document.objects);
    }
    // Verify the root dict handle actually exists in objects
    if !h.named_objects_dict_handle.is_null() && !document.objects.contains_key(&h.named_objects_dict_handle) {
        // Handle points to nonexistent object — try to find the real root dict
        h.named_objects_dict_handle = find_root_dict_handle(&document.objects);
    }

    // ── Sync child dictionary handles from root dict entries ──
    if let Some(crate::objects::ObjectType::Dictionary(root_dict)) =
        document.objects.get(&h.named_objects_dict_handle)
    {
        if let Some(handle) = root_dict.get("ACAD_GROUP") {
            h.acad_group_dict_handle = handle;
        }
        if let Some(handle) = root_dict.get("ACAD_MLINESTYLE") {
            h.acad_mlinestyle_dict_handle = handle;
        }
        if let Some(handle) = root_dict.get("ACAD_LAYOUT") {
            h.acad_layout_dict_handle = handle;
        }
        if let Some(handle) = root_dict.get("ACAD_PLOTSETTINGS") {
            h.acad_plotsettings_dict_handle = handle;
        }
        if let Some(handle) = root_dict.get("ACAD_PLOTSTYLENAME") {
            h.acad_plotstylename_dict_handle = handle;
        }
        if let Some(handle) = root_dict.get("ACAD_MATERIAL") {
            h.acad_material_dict_handle = handle;
        }
        if let Some(handle) = root_dict.get("ACAD_COLOR") {
            h.acad_color_dict_handle = handle;
        }
        if let Some(handle) = root_dict.get("ACAD_VISUALSTYLE") {
            h.acad_visualstyle_dict_handle = handle;
        }
    }

    // ── Sync linetype handles by name ──
    if let Some(lt) = document.line_types.get("ByLayer") {
        h.bylayer_linetype_handle = lt.handle;
    }
    if let Some(lt) = document.line_types.get("ByBlock") {
        h.byblock_linetype_handle = lt.handle;
    }
    if let Some(lt) = document.line_types.get("Continuous") {
        h.continuous_linetype_handle = lt.handle;
    }

    // ── Sync model/paper space block handles ──
    if let Some(br) = document.block_records.get("*Model_Space") {
        h.model_space_block_handle = br.handle;
    }
    if let Some(br) = document.block_records.get("*Paper_Space") {
        h.paper_space_block_handle = br.handle;
    }

    // ── Sync current style handles (if NULL, resolve from defaults) ──
    if h.current_layer_handle.is_null() {
        if let Some(layer) = document.layers.get("0") {
            h.current_layer_handle = layer.handle;
        }
    }
    if h.current_text_style_handle.is_null() {
        if let Some(style) = document.text_styles.get("Standard") {
            h.current_text_style_handle = style.handle;
        }
    }
    if h.current_dimstyle_handle.is_null() {
        if let Some(ds) = document.dim_styles.get("Standard") {
            h.current_dimstyle_handle = ds.handle;
        }
    }
    if h.current_linetype_handle.is_null() {
        h.current_linetype_handle = h.bylayer_linetype_handle;
    }

    // ── Correct HANDSEED ──
    let max_handle = handle_map.iter().map(|&(ha, _)| ha).max().unwrap_or(0);
    if h.handle_seed <= max_handle {
        h.handle_seed = max_handle + 1;
    }

    // ── Update model-space extents ──
    if let Some(ref ext) = extents {
        h.model_space_extents_min = ext.min;
        h.model_space_extents_max = ext.max;
    }

    h
}

/// Find the root dictionary handle by scanning the objects map.
///
/// The root dictionary is a Dictionary with `owner == Handle::NULL`.
/// If multiple candidates exist (e.g., from `initialize_defaults` and
/// from file data), prefer the one with more entries (the file's root dict).
fn find_root_dict_handle(
    objects: &std::collections::HashMap<crate::types::Handle, crate::objects::ObjectType>,
) -> crate::types::Handle {
    use crate::objects::ObjectType;
    use crate::types::Handle;

    let mut best_handle = Handle::NULL;
    let mut best_entry_count = 0usize;

    for (handle, obj) in objects {
        if let ObjectType::Dictionary(dict) = obj {
            if dict.owner.is_null() {
                // Prefer the dictionary with more entries (richer = file's root dict);
                // on tie, prefer higher handle (likely from file, not initialize_defaults)
                if dict.entries.len() > best_entry_count
                    || (dict.entries.len() == best_entry_count && handle.value() > best_handle.value())
                {
                    best_handle = *handle;
                    best_entry_count = dict.entries.len();
                }
            }
        }
    }

    best_handle
}

// ════════════════════════════════════════════════════════════════════════════
//  AC15 format (R13/R14/R2000) — linear file layout
// ════════════════════════════════════════════════════════════════════════════

fn write_ac15<W: Write + Seek>(
    output: &mut W,
    document: &CadDocument,
    version: DxfVersion,
) -> Result<()> {
    let mut fhw = DwgFileHeaderWriterAC15::new(version);

    // ── Phase 1: Compute objects FIRST to get handle map ──
    let obj_writer = DwgObjectWriter::new(document)?;
    let (obj_data, handle_map_u32, extents) = obj_writer.write();

    // ── Phase 2: Prepare header (sync handles + correct HANDSEED) ──
    let corrected_header = prepare_header(document, &handle_map_u32, &extents);

    // ── Section: Header (uses synced + corrected header) ──
    let header_data = header_writer::write_header(version, &corrected_header);
    fhw.add_section(section_names::HEADER, header_data);

    // ── Section: Classes ──
    let classes: Vec<_> = document.classes.iter().cloned().collect();
    let classes_data = classes_writer::write_classes(version, &classes);
    fhw.add_section(section_names::CLASSES, classes_data);

    // ── Section: AcDbObjects (pre-computed) ──
    fhw.add_section(section_names::ACDB_OBJECTS, obj_data);

    // ── Section: ObjFreeSpace ──
    let obj_free_space = build_obj_free_space(version, document, handle_map_u32.len());
    fhw.add_section(section_names::OBJ_FREE_SPACE, obj_free_space);

    // ── Section: Template ──
    let template = build_template();
    fhw.add_section(section_names::TEMPLATE, template);

    // ── Section: AuxHeader (uses corrected HANDSEED) ──
    let aux_data = aux_header_writer::write_aux_header(version, &corrected_header);
    fhw.add_section(section_names::AUX_HEADER, aux_data);

    // ── Section: Handles (must be last — needs objects offset) ──
    let section_offset = fhw.handle_section_offset() as i32;
    let handle_map_i64: Vec<(u64, i64)> = handle_map_u32
        .iter()
        .map(|&(h, o)| (h, o as i64))
        .collect();
    let handles_data = handle_writer::write_handles(&handle_map_i64, section_offset);
    fhw.add_section(section_names::HANDLES, handles_data);

    // ── Section: Preview ──
    let preview_data = preview_writer::write_preview(version);
    fhw.add_section(section_names::PREVIEW, preview_data);

    // ── Write final file ──
    fhw.write_file(output)?;

    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
//  AC18 format (R2004/R2010/R2013/R2018) — page-based with LZ77
// ════════════════════════════════════════════════════════════════════════════

fn write_ac18<W: Write + Seek>(
    output: &mut W,
    document: &CadDocument,
    version: DxfVersion,
) -> Result<()> {
    // AC18 writer reserves 0x100 bytes at file start for metadata
    let mut fhw = DwgFileHeaderWriterAC18::new(version, output)?;

    // R2004+ default page size for most sections
    const PAGE_SIZE: usize = 0x7400;
    // Smaller page for metadata-style sections
    const SMALL_PAGE: usize = 0x80;

    // ── Phase 1: Compute objects FIRST to get handle map ──
    let obj_writer = DwgObjectWriter::new(document)?;
    let (obj_data, handle_map_u32, extents) = obj_writer.write();

    // ── Phase 2: Prepare header (sync handles + correct HANDSEED) ──
    let corrected_header = prepare_header(document, &handle_map_u32, &extents);

    // ── Section: Header (uses synced + corrected header) ──
    let header_data = header_writer::write_header(version, &corrected_header);
    fhw.add_section(output, section_names::HEADER, &header_data, true, PAGE_SIZE)?;

    // ── Section: Classes ──
    let classes: Vec<_> = document.classes.iter().cloned().collect();
    let classes_data = classes_writer::write_classes(version, &classes);
    fhw.add_section(output, section_names::CLASSES, &classes_data, true, PAGE_SIZE)?;

    // ── Section: SummaryInfo ──
    let summary_data = build_summary_info(version);
    fhw.add_section(
        output,
        section_names::SUMMARY_INFO,
        &summary_data,
        false,
        0x100,
    )?;

    // ── Section: Preview ──
    let preview_data = preview_writer::write_preview(version);
    fhw.add_section(output, section_names::PREVIEW, &preview_data, false, 0x400)?;

    // ── Section: AppInfo ──
    let app_info_data = app_info_writer::write_app_info(version);
    fhw.add_section(output, section_names::APP_INFO, &app_info_data, false, SMALL_PAGE)?;

    // ── Section: FileDepList ──
    let file_dep_data = build_file_dep_list();
    fhw.add_section(output, section_names::FILE_DEP_LIST, &file_dep_data, false, SMALL_PAGE)?;

    // ── Section: RevHistory ──
    let rev_history_data = build_rev_history();
    fhw.add_section(output, section_names::REV_HISTORY, &rev_history_data, true, PAGE_SIZE)?;

    // ── Section: AuxHeader (uses corrected HANDSEED) ──
    let aux_data = aux_header_writer::write_aux_header(version, &corrected_header);
    fhw.add_section(output, section_names::AUX_HEADER, &aux_data, true, PAGE_SIZE)?;

    // ── Section: AcDbObjects (pre-computed) ──
    fhw.add_section(output, section_names::ACDB_OBJECTS, &obj_data, true, PAGE_SIZE)?;

    // ── Section: ObjFreeSpace ──
    let obj_free_space = build_obj_free_space(version, document, handle_map_u32.len());
    fhw.add_section(output, section_names::OBJ_FREE_SPACE, &obj_free_space, true, PAGE_SIZE)?;

    // ── Section: Template ──
    let template = build_template();
    fhw.add_section(output, section_names::TEMPLATE, &template, true, PAGE_SIZE)?;

    // ── Section: Handles (last — needs objects data) ──
    let section_offset = fhw.handle_section_offset() as i32;
    let handle_map_i64: Vec<(u64, i64)> = handle_map_u32
        .iter()
        .map(|&(h, o)| (h, o as i64))
        .collect();
    let handles_data = handle_writer::write_handles(&handle_map_i64, section_offset);
    fhw.add_section(output, section_names::HANDLES, &handles_data, true, PAGE_SIZE)?;

    // ── Write file header, section map, and page map ──
    fhw.write_file(output)?;

    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
//  AC21 format (R2007) — RS-encoded pages with LZ77 AC21 compression
// ════════════════════════════════════════════════════════════════════════════

fn write_ac21<W: Write + Seek>(
    output: &mut W,
    document: &CadDocument,
    version: DxfVersion,
) -> Result<()> {
    write_ac21_impl(output, document, version, false)
}

fn write_ac21_impl<W: Write + Seek>(
    output: &mut W,
    document: &CadDocument,
    version: DxfVersion,
    skip_lz77: bool,
) -> Result<()> {
    // AC21 writer reserves 0x480 bytes at file start (0x80 metadata + 0x400 file header)
    let mut fhw = DwgFileHeaderWriterAC21::new(version, output)?;
    fhw.skip_lz77 = skip_lz77;

    // ── Phase 1: Compute objects FIRST to get handle map ──
    let obj_writer = DwgObjectWriter::new(document)?;
    let (obj_data, handle_map_u32, extents) = obj_writer.write();

    // ── Phase 2: Prepare header (sync handles + correct HANDSEED) ──
    let corrected_header = prepare_header(document, &handle_map_u32, &extents);

    // ── Sections in spec §5.1 stream order ──
    // AC21 add_section looks up encoding/encryption/page_size automatically
    // from ac21_section_info, so no page_size or compressed flag needed.

    // SummaryInfo
    let summary_data = build_summary_info(version);
    fhw.add_section(output, section_names::SUMMARY_INFO, &summary_data)?;

    // Preview
    let preview_data = preview_writer::write_preview(version);
    fhw.add_section(output, section_names::PREVIEW, &preview_data)?;

    // AppInfo
    let app_info_data = app_info_writer::write_app_info(version);
    fhw.add_section(output, section_names::APP_INFO, &app_info_data)?;

    // FileDepList
    let file_dep_data = build_file_dep_list();
    fhw.add_section(output, section_names::FILE_DEP_LIST, &file_dep_data)?;

    // RevHistory
    let rev_history_data = build_rev_history();
    fhw.add_section(output, section_names::REV_HISTORY, &rev_history_data)?;

    // AcDbObjects (pre-computed)
    fhw.add_section(output, section_names::ACDB_OBJECTS, &obj_data)?;

    // ObjFreeSpace
    let obj_free_space = build_obj_free_space(version, document, handle_map_u32.len());
    fhw.add_section(output, section_names::OBJ_FREE_SPACE, &obj_free_space)?;

    // Template
    let template = build_template();
    fhw.add_section(output, section_names::TEMPLATE, &template)?;

    // Handles (needs objects data for offsets)
    let section_offset = fhw.handle_section_offset() as i32;
    let handle_map_i64: Vec<(u64, i64)> = handle_map_u32
        .iter()
        .map(|&(h, o)| (h, o as i64))
        .collect();
    let handles_data = handle_writer::write_handles(&handle_map_i64, section_offset);
    fhw.add_section(output, section_names::HANDLES, &handles_data)?;

    // Classes
    let classes: Vec<_> = document.classes.iter().cloned().collect();
    let classes_data = classes_writer::write_classes(version, &classes);
    fhw.add_section(output, section_names::CLASSES, &classes_data)?;

    // AuxHeader (uses corrected HANDSEED)
    let aux_data = aux_header_writer::write_aux_header(version, &corrected_header);
    fhw.add_section(output, section_names::AUX_HEADER, &aux_data)?;

    // Header (uses corrected HANDSEED)
    let header_data = header_writer::write_header(version, &corrected_header);
    fhw.add_section(output, section_names::HEADER, &header_data)?;

    // ── Finalize: section map, page map, file header, metadata ──
    fhw.write_file(output)?;

    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
//  Section data builders for simple/metadata sections
// ════════════════════════════════════════════════════════════════════════════

/// Build ObjFreeSpace section data.
///
/// Contains approximate object count and a fixed data template.
/// Matches ACadSharp's `writeObjFreeSpace`.
fn build_obj_free_space(
    version: DxfVersion,
    document: &CadDocument,
    handle_count: usize,
) -> Vec<u8> {
    let mut data = Vec::with_capacity(64);

    // Int32: 0
    data.extend_from_slice(&0i32.to_le_bytes());
    // UInt32: approximate number of objects (handles)
    data.extend_from_slice(&(handle_count as u32).to_le_bytes());

    // Julian datetime (8 bytes)
    // For simplicity, write zeros (ODA-compatible)
    if version >= DxfVersion::AC1015 {
        let _ = &document.header; // future: use TDUPDATE
    }
    data.extend_from_slice(&0i32.to_le_bytes()); // jdate
    data.extend_from_slice(&0i32.to_le_bytes()); // milli

    // UInt32: offset of objects section (0 for paged format)
    data.extend_from_slice(&0u32.to_le_bytes());

    // UInt8: number of 64-bit values that follow (ODA writes 4)
    data.push(4);
    // 4 × (u32 + u32) = 4 × 8 bytes of fixed ODA values
    data.extend_from_slice(&0x00000032u32.to_le_bytes());
    data.extend_from_slice(&0x00000000u32.to_le_bytes());
    data.extend_from_slice(&0x00000064u32.to_le_bytes());
    data.extend_from_slice(&0x00000000u32.to_le_bytes());
    data.extend_from_slice(&0x00000200u32.to_le_bytes());
    data.extend_from_slice(&0x00000000u32.to_le_bytes());
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    data.extend_from_slice(&0x00000000u32.to_le_bytes());

    data
}

/// Build Template section data.
///
/// Contains template description length (0 = no template).
/// AutoCAD reference files only write the 4-byte RL length with no
/// MEASUREMENT field when the template description is empty.
fn build_template() -> Vec<u8> {
    let mut data = Vec::with_capacity(4);
    // RL (raw long = 4 bytes): template description length (0)
    data.extend_from_slice(&0i32.to_le_bytes());
    data
}

/// Build SummaryInfo section data (AC18+ only).
///
/// Writes empty summary info fields (all empty strings).
///
/// **AC1018 (R2004)**: Windows-1252 (ANSI) strings.
///   Format: UInt16(byte_count_incl_null) + bytes + null.
///   Empty → UInt16(1) + 0x00 = 3 bytes.
///
/// **AC1021 (R2007)**: UTF-16LE strings.
///   Format: UInt16(char_count_incl_null) + UTF-16LE chars.
///   Empty → UInt16(1) + 0x00 0x00 = 4 bytes.
fn build_summary_info(version: DxfVersion) -> Vec<u8> {
    let mut data = Vec::with_capacity(128);
    let is_utf16 = version >= DxfVersion::AC1021;

    // 8 × empty strings
    // Title, Subject, Author, Keywords, Comments, LastSavedBy, RevisionNumber, HyperlinkBase
    for _ in 0..8 {
        data.extend_from_slice(&1u16.to_le_bytes()); // char/byte count including null
        if is_utf16 {
            // UTF-16LE null terminator: 2 bytes
            data.push(0);
            data.push(0);
        } else {
            // ANSI null terminator: 1 byte
            data.push(0);
        }
    }

    // Total editing time: 2 × Int32 (zeros)
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());

    // Created date: 8 bytes (zeros)
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());

    // Modified date: 8 bytes (zeros)
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());

    // Property count: Int16 (0)
    data.extend_from_slice(&0u16.to_le_bytes());

    // 2 × Int32 (trailing zeros)
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());

    data
}

/// Build FileDepList section data (AC18+ only).
///
/// Empty dependency list (0 features, 0 files).
fn build_file_dep_list() -> Vec<u8> {
    let mut data = Vec::with_capacity(8);
    // Int32: feature count (0)
    data.extend_from_slice(&0u32.to_le_bytes());
    // Int32: file count (0)
    data.extend_from_slice(&0u32.to_le_bytes());
    data
}

/// Build RevHistory section data (AC18+ only).
///
/// Empty revision history (3 × Int32 zeros).
fn build_rev_history() -> Vec<u8> {
    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes()); // revision counter
    data.extend_from_slice(&0u32.to_le_bytes());
    data
}

// ════════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::CadDocument;
    use crate::types::{DxfVersion, Handle};

    #[test]
    fn test_validate_version_r2007_ok() {
        assert!(validate_version(DxfVersion::AC1021).is_ok());
    }

    #[test]
    fn test_validate_version_unknown_rejected() {
        let result = validate_version(DxfVersion::Unknown);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_version_r2000_ok() {
        assert!(validate_version(DxfVersion::AC1015).is_ok());
    }

    #[test]
    fn test_validate_version_r2004_ok() {
        assert!(validate_version(DxfVersion::AC1018).is_ok());
    }

    #[test]
    fn test_validate_version_r2010_ok() {
        assert!(validate_version(DxfVersion::AC1024).is_ok());
    }

    #[test]
    fn test_build_template() {
        let t = build_template();
        assert_eq!(t.len(), 4);
        assert_eq!(i32::from_le_bytes([t[0], t[1], t[2], t[3]]), 0); // desc length (RL = 4 bytes)
    }

    #[test]
    fn test_build_obj_free_space() {
        let doc = CadDocument::new();
        let data = build_obj_free_space(DxfVersion::AC1015, &doc, 42);
        // Int32(0) + UInt32(42) + 8 date + UInt32(0) + 1 + 32
        assert_eq!(data.len(), 4 + 4 + 8 + 4 + 1 + 32);
        // Check handle count at offset 4
        let count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        assert_eq!(count, 42);
    }

    #[test]
    fn test_build_file_dep_list() {
        let d = build_file_dep_list();
        assert_eq!(d.len(), 8);
    }

    #[test]
    fn test_build_rev_history() {
        let d = build_rev_history();
        assert_eq!(d.len(), 16);
    }

    #[test]
    fn test_build_summary_info_ac18() {
        let d = build_summary_info(DxfVersion::AC1018);
        // 8 × 3 bytes (u16(1) + ANSI null) + 8 + 16 + 2 + 8 = 58
        assert_eq!(d.len(), 58);
        assert_eq!(u16::from_le_bytes([d[0], d[1]]), 1);
        assert_eq!(d[2], 0);
        // Next string starts at offset 3
        assert_eq!(u16::from_le_bytes([d[3], d[4]]), 1);
    }

    #[test]
    fn test_build_summary_info_ac21() {
        let d = build_summary_info(DxfVersion::AC1021);
        // 8 × 4 bytes (u16(1) + UTF-16LE null) + 8 + 16 + 2 + 8 = 66
        assert_eq!(d.len(), 66);
        // First string: u16(1) + 00 00
        assert_eq!(u16::from_le_bytes([d[0], d[1]]), 1);
        assert_eq!(d[2], 0);
        assert_eq!(d[3], 0);
        // Next string starts at offset 4
        assert_eq!(u16::from_le_bytes([d[4], d[5]]), 1);
    }

    #[test]
    fn test_write_to_vec_r2000() {
        let mut doc = CadDocument::new();
        doc.version = DxfVersion::AC1015;
        let result = DwgWriter::write_to_vec(&doc);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        // AC15 file header magic: "AC1015"
        assert!(bytes.len() > 100);
        let magic = std::str::from_utf8(&bytes[0..6]).unwrap_or("");
        assert_eq!(magic, "AC1015");
    }

    #[test]
    fn test_write_to_vec_r2004() {
        let mut doc = CadDocument::new();
        doc.version = DxfVersion::AC1018;
        let result = DwgWriter::write_to_vec(&doc);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(bytes.len() > 0x100);
        // AC18 magic at offset 0
        let magic = std::str::from_utf8(&bytes[0..6]).unwrap_or("");
        assert_eq!(magic, "AC1018");
    }

    #[test]
    fn test_write_to_vec_r2010() {
        let mut doc = CadDocument::new();
        doc.version = DxfVersion::AC1024;
        let result = DwgWriter::write_to_vec(&doc);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(bytes.len() > 0x100);
    }

    #[test]
    fn test_write_to_vec_r2013() {
        let mut doc = CadDocument::new();
        doc.version = DxfVersion::AC1027;
        let result = DwgWriter::write_to_vec(&doc);
        assert!(result.is_ok());
    }

    #[test]
    fn test_write_to_vec_r2018() {
        let mut doc = CadDocument::new();
        doc.version = DxfVersion::AC1032;
        let result = DwgWriter::write_to_vec(&doc);
        assert!(result.is_ok());
    }

    #[test]
    fn test_write_to_vec_r2007() {
        let mut doc = CadDocument::new();
        doc.version = DxfVersion::AC1021;
        let result = DwgWriter::write_to_vec(&doc);
        assert!(result.is_ok(), "AC1021 writing should succeed");
        let bytes = result.unwrap();
        assert!(bytes.len() > 0x480, "AC1021 file should be larger than header");
        let magic = std::str::from_utf8(&bytes[0..6]).unwrap_or("");
        assert_eq!(magic, "AC1021");
    }

    #[test]
    fn test_uses_ac21_format() {
        assert!(uses_ac21_format(DxfVersion::AC1021));
        assert!(!uses_ac21_format(DxfVersion::AC1018));
        assert!(!uses_ac21_format(DxfVersion::AC1024));
        assert!(!uses_ac21_format(DxfVersion::AC1015));
    }

    #[test]
    fn test_write_to_vec_r14() {
        let mut doc = CadDocument::new();
        doc.version = DxfVersion::AC1014;
        let result = DwgWriter::write_to_vec(&doc);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        let magic = std::str::from_utf8(&bytes[0..6]).unwrap_or("");
        assert_eq!(magic, "AC1014");
    }

    #[test]
    fn test_roundtrip_file_write() {
        let doc = CadDocument::new();
        let mut doc2 = doc.clone();
        doc2.version = DxfVersion::AC1015;

        let bytes = DwgWriter::write_to_vec(&doc2).unwrap();
        // Verify non-trivial output
        assert!(bytes.len() > 200, "DWG file should be non-trivial");
    }

    #[test]
    fn test_prepare_header_syncs_null_handles() {
        // Simulate the bug: create a document and zero out all header handles
        // (as would happen after reading a DWG with a broken header reader).
        let mut doc = CadDocument::new();
        doc.version = DxfVersion::AC1015;

        // Save correct handles for later verification
        let correct_block_control = doc.block_records.handle();
        let correct_layer_control = doc.layers.handle();
        let correct_style_control = doc.text_styles.handle();
        let correct_ltype_control = doc.line_types.handle();
        let correct_view_control = doc.views.handle();
        let correct_ucs_control = doc.ucss.handle();
        let correct_vport_control = doc.vports.handle();
        let correct_appid_control = doc.app_ids.handle();
        let correct_dimstyle_control = doc.dim_styles.handle();
        let correct_root_dict = doc.header.named_objects_dict_handle;

        // Zero out all header handles (simulate the reader bug)
        doc.header.block_control_handle = Handle::NULL;
        doc.header.layer_control_handle = Handle::NULL;
        doc.header.style_control_handle = Handle::NULL;
        doc.header.linetype_control_handle = Handle::NULL;
        doc.header.view_control_handle = Handle::NULL;
        doc.header.ucs_control_handle = Handle::NULL;
        doc.header.vport_control_handle = Handle::NULL;
        doc.header.appid_control_handle = Handle::NULL;
        doc.header.dimstyle_control_handle = Handle::NULL;
        doc.header.named_objects_dict_handle = Handle::NULL;
        doc.header.acad_group_dict_handle = Handle::NULL;
        doc.header.acad_mlinestyle_dict_handle = Handle::NULL;
        doc.header.acad_layout_dict_handle = Handle::NULL;
        doc.header.bylayer_linetype_handle = Handle::NULL;
        doc.header.byblock_linetype_handle = Handle::NULL;
        doc.header.continuous_linetype_handle = Handle::NULL;
        doc.header.current_layer_handle = Handle::NULL;
        doc.header.current_text_style_handle = Handle::NULL;
        doc.header.current_dimstyle_handle = Handle::NULL;
        doc.header.current_linetype_handle = Handle::NULL;

        // prepare_header should sync all handles from document objects
        let handle_map = vec![(1u64, 0u32), (2, 100), (3, 200)]; // dummy
        let prepared = prepare_header(&doc, &handle_map, &None);

        // Table control handles must be synced from the actual table objects
        assert_eq!(prepared.block_control_handle, correct_block_control,
            "block_control_handle should be synced from block_records.handle()");
        assert_eq!(prepared.layer_control_handle, correct_layer_control,
            "layer_control_handle should be synced from layers.handle()");
        assert_eq!(prepared.style_control_handle, correct_style_control,
            "style_control_handle should be synced from text_styles.handle()");
        assert_eq!(prepared.linetype_control_handle, correct_ltype_control,
            "linetype_control_handle should be synced from line_types.handle()");
        assert_eq!(prepared.view_control_handle, correct_view_control,
            "view_control_handle should be synced from views.handle()");
        assert_eq!(prepared.ucs_control_handle, correct_ucs_control,
            "ucs_control_handle should be synced from ucss.handle()");
        assert_eq!(prepared.vport_control_handle, correct_vport_control,
            "vport_control_handle should be synced from vports.handle()");
        assert_eq!(prepared.appid_control_handle, correct_appid_control,
            "appid_control_handle should be synced from app_ids.handle()");
        assert_eq!(prepared.dimstyle_control_handle, correct_dimstyle_control,
            "dimstyle_control_handle should be synced from dim_styles.handle()");

        // Root dictionary must be found
        assert_eq!(prepared.named_objects_dict_handle, correct_root_dict,
            "named_objects_dict_handle should be found by scanning objects");
        assert!(!prepared.named_objects_dict_handle.is_null(),
            "named_objects_dict_handle must not be NULL");

        // Dict handles from root dict entries must be resolved
        assert!(!prepared.acad_group_dict_handle.is_null(),
            "acad_group_dict_handle must be resolved from root dict");
        assert!(!prepared.acad_mlinestyle_dict_handle.is_null(),
            "acad_mlinestyle_dict_handle must be resolved from root dict");
        assert!(!prepared.acad_layout_dict_handle.is_null(),
            "acad_layout_dict_handle must be resolved from root dict");

        // Linetype handles must be resolved
        assert!(!prepared.bylayer_linetype_handle.is_null(),
            "bylayer_linetype_handle must be resolved");
        assert!(!prepared.byblock_linetype_handle.is_null(),
            "byblock_linetype_handle must be resolved");
        assert!(!prepared.continuous_linetype_handle.is_null(),
            "continuous_linetype_handle must be resolved");

        // Current style handles must be resolved
        assert!(!prepared.current_layer_handle.is_null(),
            "current_layer_handle must be resolved");
        assert!(!prepared.current_text_style_handle.is_null(),
            "current_text_style_handle must be resolved");
        assert!(!prepared.current_dimstyle_handle.is_null(),
            "current_dimstyle_handle must be resolved");
        assert!(!prepared.current_linetype_handle.is_null(),
            "current_linetype_handle must be resolved (default to ByLayer)");
    }

    #[test]
    fn test_prepare_header_null_handles_write_produces_valid_dwg() {
        // Simulate bug and verify the written DWG file is still valid
        let mut doc = CadDocument::new();
        doc.version = DxfVersion::AC1015;

        // Zero out all header handles
        doc.header.block_control_handle = Handle::NULL;
        doc.header.layer_control_handle = Handle::NULL;
        doc.header.named_objects_dict_handle = Handle::NULL;

        // Writing should succeed (prepare_header syncs handles)
        let result = DwgWriter::write_to_vec(&doc);
        assert!(result.is_ok(), "Writing with NULL headers should succeed after sync");
        let bytes = result.unwrap();
        assert!(bytes.len() > 200, "Output should be non-trivial");
    }
}
