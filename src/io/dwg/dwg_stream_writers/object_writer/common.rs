//! Common object-writing helpers
//!
//! These methods form the shared infrastructure for writing every DWG
//! object record: CRC wrapping, size encoding, entity/non-entity
//! preambles, extended-data emission, and handle-map registration.

use crate::io::dwg::crc;
use crate::io::dwg::dwg_reference_type::DwgReferenceType;
use crate::types::{Color, Handle, Transparency};

use super::DwgObjectWriter;

// ── DWG fixed object-type codes ─────────────────────────────────────
/// Standard DWG object type codes for entities and table entries.
pub const OBJ_TEXT: i16 = 1;
pub const OBJ_ATTRIB: i16 = 2;
pub const OBJ_ATTDEF: i16 = 3;
pub const OBJ_BLOCK: i16 = 4;
pub const OBJ_ENDBLK: i16 = 5;
pub const OBJ_SEQEND: i16 = 6;
pub const OBJ_INSERT: i16 = 7;
pub const OBJ_MINSERT: i16 = 8;
pub const OBJ_VERTEX_2D: i16 = 10;
pub const OBJ_VERTEX_3D: i16 = 11;
pub const OBJ_VERTEX_MESH: i16 = 12;
pub const OBJ_VERTEX_PFACE: i16 = 13;
pub const OBJ_VERTEX_PFACE_FACE: i16 = 14;
pub const OBJ_POLYLINE_2D: i16 = 15;
pub const OBJ_POLYLINE_3D: i16 = 16;
pub const OBJ_ARC: i16 = 17;
pub const OBJ_CIRCLE: i16 = 18;
pub const OBJ_LINE: i16 = 19;
pub const OBJ_DIMENSION_ORDINATE: i16 = 20;
pub const OBJ_DIMENSION_LINEAR: i16 = 21;
pub const OBJ_DIMENSION_ALIGNED: i16 = 22;
pub const OBJ_DIMENSION_ANG_3PT: i16 = 23;
pub const OBJ_DIMENSION_ANG_2LN: i16 = 24;
pub const OBJ_DIMENSION_RADIUS: i16 = 25;
pub const OBJ_DIMENSION_DIAMETER: i16 = 26;
pub const OBJ_POINT: i16 = 27;
pub const OBJ_3DFACE: i16 = 28;
pub const OBJ_POLYLINE_PFACE: i16 = 29;
pub const OBJ_POLYLINE_MESH: i16 = 30;
pub const OBJ_SOLID: i16 = 31;
pub const OBJ_TRACE: i16 = 32;
pub const OBJ_SHAPE: i16 = 33;
pub const OBJ_VIEWPORT: i16 = 34;
pub const OBJ_ELLIPSE: i16 = 35;
pub const OBJ_SPLINE: i16 = 36;
pub const OBJ_REGION: i16 = 37;
pub const OBJ_3DSOLID: i16 = 38;
pub const OBJ_BODY: i16 = 39;
pub const OBJ_RAY: i16 = 40;
pub const OBJ_XLINE: i16 = 41;
pub const OBJ_DICTIONARY: i16 = 42;
pub const OBJ_OLEFRAME: i16 = 43;
pub const OBJ_MTEXT: i16 = 44;
pub const OBJ_LEADER: i16 = 45;
pub const OBJ_TOLERANCE: i16 = 46;
pub const OBJ_MLINE: i16 = 47;
pub const OBJ_BLOCK_CONTROL: i16 = 48;
pub const OBJ_BLOCK_HEADER: i16 = 49;
pub const OBJ_LAYER_CONTROL: i16 = 50;
pub const OBJ_LAYER: i16 = 51;
pub const OBJ_STYLE_CONTROL: i16 = 52;
pub const OBJ_STYLE: i16 = 53;
pub const OBJ_LTYPE_CONTROL: i16 = 56;
pub const OBJ_LTYPE: i16 = 57;
pub const OBJ_VIEW_CONTROL: i16 = 60;
pub const OBJ_VIEW: i16 = 61;
pub const OBJ_UCS_CONTROL: i16 = 62;
pub const OBJ_UCS: i16 = 63;
pub const OBJ_VPORT_CONTROL: i16 = 64;
pub const OBJ_VPORT: i16 = 65;
pub const OBJ_APPID_CONTROL: i16 = 66;
pub const OBJ_APPID: i16 = 67;
pub const OBJ_DIMSTYLE_CONTROL: i16 = 68;
pub const OBJ_DIMSTYLE: i16 = 69;
pub const OBJ_GROUP: i16 = 72;
pub const OBJ_MLINESTYLE: i16 = 73;
pub const OBJ_OLE2FRAME: i16 = 74;

// Non-fixed type codes (variable, written as class index + 500)
// For simplicity in the writer, we use negative sentinel values
// and handle them in write_common_data via write_object_type.
pub const OBJ_LWPOLYLINE: i16 = 77;    // class-based in some versions
pub const OBJ_HATCH: i16 = 78;
pub const OBJ_IMAGE: i16 = 79;
pub const OBJ_MESH: i16 = 80;
pub const OBJ_MULTILEADER: i16 = 81;

// Fixed-type non-graphical objects (standard type codes from ODA spec)
pub const OBJ_XRECORD: i16 = 79;        // 0x4F
pub const OBJ_PLACEHOLDER: i16 = 80;    // 0x50
pub const OBJ_LAYOUT: i16 = 82;         // 0x52 (R2004+; for R2004Pre use class number)

// Class-based (variable) non-graphical objects:
// These are UNLISTED in C# — for R2004+ the standard type code works,
// but for pre-R2004 they should use the DXF class number.
// We use the ODA-documented type codes that work for R2004+.
pub const OBJ_DICTIONARYWDFLT: i16 = 0x78;  // 120 (class-based)
pub const OBJ_DICTIONARYVAR: i16 = 0x79;    // 121
pub const OBJ_PLOTSETTINGS: i16 = 0x7A;     // 122
pub const OBJ_MLEADERSTYLE: i16 = 0x7B;     // 123
pub const OBJ_IMAGEDEF: i16 = 0x7C;         // 124
pub const OBJ_IMAGEDEFREACTOR: i16 = 0x7D;  // 125
pub const OBJ_SCALE: i16 = 0x7E;            // 126
pub const OBJ_SORTENTSTABLE: i16 = 0x7F;    // 127
pub const OBJ_RASTERVARIABLES: i16 = 0x80;  // 128
pub const OBJ_DBCOLOR: i16 = 0x81;          // 129
pub const OBJ_WIPEOUTVARIABLES: i16 = 0x82; // 130

// ── Methods on DwgObjectWriter ──────────────────────────────────────
impl<'a> DwgObjectWriter<'a> {
    // ── register_object ─────────────────────────────────────────────
    /// Finalise the current object record in `self.writer` and append it
    /// to the output stream, recording the handle→offset mapping.
    ///
    /// Record structure (per ODA spec §20.2):
    /// ```text
    /// [ModularShort(size)][R2010+: ModularChar(handle_bits)][merged_data][CRC16]
    /// ```
    ///
    /// **CRC-16** (seed `0xC0C1`) covers everything from the ModularShort
    /// through the end of `merged_data`.  AutoCAD validates this checksum
    /// on every object load ("Level 3" check).
    ///
    /// **Byte alignment**: The merged data from `merge()` is guaranteed
    /// byte-aligned (all three sub-streams are spear-shifted and the
    /// final output is flushed).  This is critical because the CRC-16
    /// operates on complete bytes.
    pub fn register_object(&mut self, handle: Handle) {
        // 1. Merge all sub-streams → single byte buffer
        //    (handle_start_bits is recorded during merge by the merged writer)
        let data = self.writer.merge();

        // Verify the merged data is byte-aligned.  If this fires, the
        // merge function has a bug — partial bytes would corrupt the
        // CRC-16 and cause "Invalid Input" in AutoCAD.
        debug_assert!(
            !data.is_empty() || handle.is_null(),
            "register_object: merged data is empty for handle {:#X}",
            handle.value()
        );

        // 2. Compute handle-stream bit count for R2010+ MC header field.
        // Must be computed AFTER merge: handle_bits = total_bits - handle_start.
        // This matches C#: sizeb = (msmain.Length << 3) - SavedPositionInBits
        let handle_bits = if self.version.r2010_plus() {
            let total_bits = (data.len() as i64) * 8;
            let hstart = self.writer.handle_start_bits();
            debug_assert!(
                hstart >= 0 && hstart <= total_bits,
                "handle_start_bits ({}) out of range [0, {}]",
                hstart,
                total_bits
            );
            total_bits - hstart
        } else {
            0
        };

        // 3. Build output record
        let pos = self.output.len() as u32;

        // 3a. Size (modular short) — the merged data byte count.
        //     Per the ODA spec, this is the byte count of the data area
        //     (includes object data AND handle reference data).
        write_modular_short_bytes(&mut self.output, data.len());

        // 3b. R2010+: handle stream size in bits (modular char)
        if self.version.r2010_plus() {
            write_modular_char_bytes(&mut self.output, handle_bits as usize);
        }

        // 3c. Merged data (byte-aligned, complete object record)
        self.output.extend_from_slice(&data);

        // 4. CRC-16 over everything from `pos`:
        //    [MS(size)] [MC(handle_bits)] [merged_data]
        //    Seed 0xC0C1, no final XOR.  This is the per-object checksum
        //    that AutoCAD validates ("Level 3" object integrity check).
        let crc_val = crc::crc16(crc::CRC16_SEED, &self.output[pos as usize..]);
        self.output.extend_from_slice(&crc_val.to_le_bytes());

        // 5. Record handle → byte-offset mapping.
        //    These offsets are relative to the start of the AcDbObjects
        //    section data (which includes the 0x0DCA marker for R2004+).
        //    The Handles section (Object Map / Address Table) uses these
        //    offsets for handle → object lookup.
        if !handle.is_null() {
            self.handle_map.push((handle.value(), pos));
        }

        // 6. Reset per-object writer for the next object
        self.writer.reset();
    }

    // ── write_common_data ───────────────────────────────────────────
    /// Object type + handle + extended-data preamble shared by
    /// every object (entities AND non-graphical objects).
    pub fn write_common_data(
        &mut self,
        type_code: i16,
        handle: Handle,
        xdata: &crate::xdata::ExtendedData,
    ) {
        // Object type (BS or MC depending on version)
        self.writer.write_object_type(type_code);

        // R2000..R2007: save position for deferred size field
        if self.version.r2000_plus() && !self.version.r2010_plus() {
            self.writer.save_position_for_size();
        }

        // Handle (absolute)
        self.writer.main_mut().write_handle_undefined(handle.value());

        // Extended data
        self.write_extended_data(xdata);
    }

    // ── write_common_entity_data ────────────────────────────────────
    /// Full preamble for an entity: type code, handle, xdata, graphic
    /// flag, entity mode, reactors/xdic, layer/linetype, colour,
    /// line-weight, prev/next entity chain.
    ///
    /// Field order must match the C# reference exactly because both the
    /// main-stream and handle-stream are order-sensitive.
    pub fn write_common_entity_data(
        &mut self,
        type_code: i16,
        handle: Handle,
        owner_handle: Handle,
        layer: &str,
        color: &Color,
        line_weight: &crate::types::LineWeight,
        transparency: &Transparency,
        invisible: bool,
        xdata: &crate::xdata::ExtendedData,
        reactors: &[Handle],
        xdictionary_handle: &Option<Handle>,
    ) {
        // ── MAIN + HANDLE: shared preamble (type + handle + xdata) ──
        self.write_common_data(type_code, handle, xdata);

        // ── MAIN: graphic presence flag ──
        self.writer.write_bit(false);

        // ── MAIN: R13-R14 save position for size ──
        if self.version.r13_14_only() {
            self.writer.save_position_for_size();
        }

        // ── MAIN: entity mode (2 bits) ──
        let entmode = self.get_entity_mode(&owner_handle);
        self.writer.write_2bits(entmode);

        // ── HANDLE: owner (if entmode == 0) ──
        if entmode == 0 {
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, owner_handle.value());
        }

        // ── MAIN + HANDLE: reactors + xdic ──
        // Reactor count (MAIN)
        self.writer.write_bit_long(reactors.len() as i32);
        // Reactor handles (HANDLE)
        for r in reactors {
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, r.value());
        }

        // R2004+: no-xdic flag (MAIN) + conditional xdic handle (HANDLE)
        // Pre-R2004: always write xdic handle (0 if none)
        if self.version.r2004_plus() {
            self.writer.write_bit(xdictionary_handle.is_none());
            // Xdic handle (HANDLE) — only if present
            if let Some(xdic) = xdictionary_handle {
                self.writer
                    .write_handle(DwgReferenceType::HardOwnership, xdic.value());
            }
        } else {
            // Pre-R2004: always write xdic handle (0 if none)
            let xdic_val = xdictionary_handle
                .map(|h| h.value())
                .unwrap_or(0);
            self.writer
                .write_handle(DwgReferenceType::HardOwnership, xdic_val);
        }

        // R2013+: binary-data-present flag (MAIN)
        if self.version.r2013_plus(self.dxf_version) {
            self.writer.write_bit(false);
        }

        // ── R13-R14 only: layer + linetype ──
        if self.version.r13_14_only() {
            // Layer handle (HANDLE: hard pointer)
            let layer_h = self
                .document
                .layers
                .get(layer)
                .map(|l| l.handle)
                .unwrap_or(Handle::NULL);
            self.writer
                .write_handle(DwgReferenceType::HardPointer, layer_h.value());

            // Isbylayerlt flag (MAIN)
            self.writer.write_bit(true); // simplified: always by-layer
            // If not by-layer, would write linetype handle here
        }

        // ── Pre-R2004: Nolinks + prev/next ──
        if !self.version.r2004_plus() && self.version.r2000_plus() {
            // Nolinks optimization: if prev == handle-1 and next == handle+1,
            // write Nolinks=true and skip handles
            let prev_h = self.prev_handle.unwrap_or(Handle::NULL);
            let next_h = self.next_handle.unwrap_or(Handle::NULL);
            let has_links = !prev_h.is_null()
                && prev_h.value() == handle.value().wrapping_sub(1)
                && !next_h.is_null()
                && next_h.value() == handle.value().wrapping_add(1);

            // MAIN: Nolinks bit (true = sequential links, skip handles)
            self.writer.write_bit(has_links);

            if !has_links {
                // HANDLE: prev + next entity handles
                self.writer
                    .write_handle(DwgReferenceType::SoftPointer, prev_h.value());
                self.writer
                    .write_handle(DwgReferenceType::SoftPointer, next_h.value());
            }
        }

        // ── MAIN: Color (EnColor) ──
        if self.version.r2000_plus() {
            self.writer.write_en_color(color, transparency);
        } else {
            // R13-R14: colour as CMC
            self.writer.write_cm_color(color);
        }

        // ── MAIN: Linetype scale ──
        self.writer.write_bit_double(1.0); // simplified: always 1.0

        // ── R13-R14 only: invisibility + early return ──
        if self.version.r13_14_only() {
            self.writer.write_bit_short(if invisible { 1 } else { 0 });
            return;
        }

        // ── R2000+: Layer handle (HANDLE: soft pointer, spec §19.3.2) ──
        let layer_h = self
            .document
            .layers
            .get(layer)
            .map(|l| l.handle)
            .unwrap_or(Handle::NULL);
        self.writer
            .write_handle(DwgReferenceType::SoftPointer, layer_h.value());

        // ── MAIN: Linetype flags ──
        // 00 = bylayer, 01 = byblock, 10 = continuous, 11 = handle present
        self.writer.write_2bits(0b00); // simplified: always by-layer

        // ── R2007+: material flags + shadow flags ──
        if self.version.r2007_plus() {
            // Material flags BB (00 = by layer)
            self.writer.write_2bits(0b00);
            // Shadow flags RC
            self.writer.write_byte(0);
        }

        // ── R2000+: Plotstyle flags ──
        self.writer.write_2bits(0b00); // simplified: always by-layer

        // ── R2007+ (>AC1021): visual style bits ──
        if self.version.r2010_plus() {
            self.writer.write_bit(false); // has full visual style
            self.writer.write_bit(false); // has face visual style
            self.writer.write_bit(false); // has edge visual style
        }

        // ── MAIN: Invisibility ──
        self.writer.write_bit_short(if invisible { 1 } else { 0 });

        // ── R2000+: Lineweight ──
        let lw_val = line_weight.as_i16();
        self.writer.write_byte(lw_val as u8);
    }

    // ── write_common_non_entity_data ────────────────────────────────
    /// Preamble for non-entity objects: type code, handle,
    /// owner handle, reactors, xdictionary.
    pub fn write_common_non_entity_data(
        &mut self,
        type_code: i16,
        handle: Handle,
        owner_handle: Handle,
        reactors: &[Handle],
        xdictionary_handle: &Option<Handle>,
    ) {
        // ── writeCommonData portion ──

        // Object type
        self.writer.write_object_type(type_code);

        // R2000..R2007: size placeholder (AC1015..AC1021)
        // C#: this._version >= AC1015 && this._version < AC1024
        // R2010 (AC1024) does NOT get a size placeholder.
        if self.version.r2000_plus() && !self.version.r2010_plus() {
            self.writer.save_position_for_size();
        }

        // Handle (absolute)
        self.writer.main_mut().write_handle_undefined(handle.value());

        // Extended data — empty for non-entities
        let empty = crate::xdata::ExtendedData::default();
        self.write_extended_data(&empty);

        // ── R13-R14 Only: size placeholder (after xdata, before owner) ──
        if self.version.r13_14_only() {
            self.writer.save_position_for_size();
        }

        // ── HANDLE: Owner handle (soft pointer) ──
        self.writer
            .write_handle(DwgReferenceType::SoftPointer, owner_handle.value());

        // ── writeReactorsAndDictionaryHandle portion ──

        // MAIN: Reactor count
        self.writer.write_bit_long(reactors.len() as i32);

        // HANDLE: Reactor handles
        for r in reactors {
            self.writer
                .write_handle(DwgReferenceType::SoftPointer, r.value());
        }

        // R2004+: MAIN no-xdic flag + conditional HANDLE xdic
        // Pre-R2004: HANDLE xdic always written (0 if null)
        let no_xdic = xdictionary_handle.is_none();
        if self.version.r2004_plus() {
            self.writer.write_bit(no_xdic);
            if !no_xdic {
                self.writer.write_handle(
                    DwgReferenceType::HardOwnership,
                    xdictionary_handle.unwrap().value(),
                );
            }
        } else {
            // Pre-R2004: always emit xdic handle (0 if None)
            let xdic_val = xdictionary_handle
                .map(|h| h.value())
                .unwrap_or(0);
            self.writer
                .write_handle(DwgReferenceType::HardOwnership, xdic_val);
        }

        // R2013+: binary-data flag
        if self.version.r2013_plus(self.dxf_version) {
            self.writer.write_bit(false);
        }
    }

    // ── write_xref_dependant_bit ────────────────────────────────────
    /// 64-group "xref dependant" flag (always false for now).
    pub fn write_xref_dependant_bit(&mut self) {
        if self.version.r2007_plus() {
            // R2007+: xrefindex+1 BS 70 (combined flags)
            self.writer.write_bit_short(0);
        } else {
            // Pre-R2007: 64-flag B (Referenced), xrefindex+1 BS, Xdep B (XrefDependent)
            self.writer.write_bit(false); // referenced flag
            self.writer.write_bit_short(0); // xrefindex+1
            self.writer.write_bit(false); // xref dependent flag
        }
    }

    // ── write_extended_data ─────────────────────────────────────────
    /// Write registered-application extended data (XDATA) blocks.
    /// For now, writes a zero-count, meaning "no xdata".
    pub fn write_extended_data(&mut self, _xdata: &crate::xdata::ExtendedData) {
        // EED size terminator: BS 0 = no more xdata applications
        self.writer.write_bit_short(0);
    }

    // ── sub-entity handle allocator ────────────────────────────────
    /// Allocate a new unique handle for sub-entities (vertices, seqend)
    /// that don't have handles assigned by the document.
    pub fn alloc_handle(&mut self) -> Handle {
        let h = self.next_alloc_handle;
        self.next_alloc_handle += 1;
        Handle::new(h)
    }

    // ── class_type_code ─────────────────────────────────────────────
    /// Look up the DXF class number for an UNLISTED object type.
    ///
    /// In C# ACadSharp, types not in the `ObjectType` enum (UNLISTED)
    /// **always** use their DXF class number (500+) as the DWG type
    /// code — regardless of version.  Only fixed types (0–82 in the
    /// ODA spec) use literal type codes.
    pub fn class_type_code(&self, dxf_name: &str, fallback: i16) -> i16 {
        self.document
            .classes
            .get_by_name(dxf_name)
            .map(|c| c.class_number)
            .unwrap_or(fallback)
    }

    // ── entity-mode helper ──────────────────────────────────────────
    /// Returns the 2-bit entity-mode value:
    /// - 0 = owned (owner handle present) — VERTEX, ATTRIB, SEQEND, etc.
    /// - 1 = paper-space block
    /// - 2 = model-space block
    /// - 3 = (unused)
    fn get_entity_mode(&self, owner_handle: &Handle) -> u8 {
        // Check if owner is model-space or paper-space block record
        let ms_handle = self
            .document
            .block_records
            .get("*Model_Space")
            .map(|br| br.handle);
        let ps_handle = self
            .document
            .block_records
            .get("*Paper_Space")
            .map(|br| br.handle);

        if let Some(ms) = ms_handle {
            if *owner_handle == ms {
                return 2;
            }
        }
        if let Some(ps) = ps_handle {
            if *owner_handle == ps {
                return 1;
            }
        }
        0
    }
}

// ── Helper: write modular-short to a byte vec ───────────────────────
/// Encode `value` as a DWG modular short (MS) and append to `output`.
/// Each 16-bit word carries 15 data bits; bit 15 = continuation flag.
pub(crate) fn write_modular_short_bytes(output: &mut Vec<u8>, value: usize) {
    let mut remaining = value;
    loop {
        let word = (remaining & 0x7FFF) as u16;
        remaining >>= 15;
        if remaining > 0 {
            output.extend_from_slice(&(word | 0x8000).to_le_bytes());
        } else {
            output.extend_from_slice(&word.to_le_bytes());
            break;
        }
    }
}

/// Encode `value` as a DWG modular char (MC) and append to `output`.
/// Each byte carries 7 data bits; bit 7 = continuation flag.
/// This is used for R2010+ handle-stream bit count in the record header.
pub(crate) fn write_modular_char_bytes(output: &mut Vec<u8>, value: usize) {
    if value == 0 {
        output.push(0);
        return;
    }
    let mut remaining = value;
    while remaining > 0 {
        let b = (remaining & 0x7F) as u8;
        remaining >>= 7;
        if remaining > 0 {
            output.push(b | 0x80);
        } else {
            output.push(b);
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modular_short_small() {
        let mut buf = Vec::new();
        write_modular_short_bytes(&mut buf, 100);
        assert_eq!(buf.len(), 2);
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), 100);
    }

    #[test]
    fn modular_short_large() {
        let mut buf = Vec::new();
        // 0x8000 requires two words
        write_modular_short_bytes(&mut buf, 0x8000);
        assert_eq!(buf.len(), 4);
        // First word has continuation flag
        let w0 = u16::from_le_bytes([buf[0], buf[1]]);
        assert_ne!(w0 & 0x8000, 0);
        // Second word has no flag
        let w1 = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(w1 & 0x8000, 0);
        // Reconstruct
        let lo = (w0 & 0x7FFF) as usize;
        let hi = (w1 & 0x7FFF) as usize;
        assert_eq!(lo | (hi << 15), 0x8000);
    }

    #[test]
    fn modular_short_zero() {
        let mut buf = Vec::new();
        write_modular_short_bytes(&mut buf, 0);
        assert_eq!(buf.len(), 2);
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), 0);
    }

    #[test]
    fn modular_short_max_single_word() {
        let mut buf = Vec::new();
        write_modular_short_bytes(&mut buf, 0x7FFF);
        assert_eq!(buf.len(), 2);
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), 0x7FFF);
    }
}
