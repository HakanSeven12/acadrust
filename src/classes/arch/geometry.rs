//! Geometry in Arch proxy graphics and measured native wall fields.

use super::ArchEntityData;
use crate::entities::{ProxyGraphicRecord, ProxyGraphics};
use crate::io::dwg::dwg_stream_readers::{
    merged_reader::DwgMergedReader, object_reader::DwgObjectReader,
};
use crate::types::{DxfVersion, Vector3};

const POINT_BYTES: usize = 24;
const MAX_POINTS: usize = 100_000;

/// Decode a type-6 profile payload: `u32` count, then `count` XYZ doubles.
pub fn polyline_from_type6(data: &[u8]) -> Option<Vec<Vector3>> {
    let count = u32::from_le_bytes(data.get(..4)?.try_into().ok()?) as usize;
    if !(2..=MAX_POINTS).contains(&count) {
        return None;
    }
    let needed = 4usize.checked_add(count.checked_mul(POINT_BYTES)?)?;
    if needed > data.len() {
        return None;
    }
    (0..count)
        .map(|index| {
            let offset = 4 + index * POINT_BYTES;
            let point = Vector3::new(
                read_f64(data, offset)?,
                read_f64(data, offset + 8)?,
                read_f64(data, offset + 16)?,
            );
            point_is_finite(point).then_some(point)
        })
        .collect()
}

/// Profiles carried by an entity's proxy-graphics blob.
pub fn decode_profiles(graphic_data: Option<&[u8]>) -> Vec<Vec<Vector3>> {
    let Some(graphics) = graphic_data.and_then(ProxyGraphics::decode) else {
        return Vec::new();
    };
    graphics
        .records
        .into_iter()
        .filter_map(|record| match record {
            ProxyGraphicRecord::Unknown {
                record_type: 6,
                data,
            } => polyline_from_type6(&data),
            _ => None,
        })
        .collect()
}

/// Map complete encoded profile runs, without scanning for isolated coordinate values.
///
/// Matches include the count and every point. Replacements are collected before
/// writing, so a new coordinate cannot become an input to a later replacement.
pub fn map_profile_points(
    buffers: &mut [&mut [u8]],
    profiles: &[Vec<Vector3>],
    mut map: impl FnMut(Vector3) -> Vector3,
) -> usize {
    let mut mappings = Vec::new();
    for profile in profiles {
        let mapped: Vec<_> = profile.iter().copied().map(&mut map).collect();
        if mapped.iter().any(|point| !point_is_finite(*point)) {
            return 0;
        }
        mappings.push((
            encode_profile(profile),
            encode_profile(&mapped),
            profile.len(),
        ));
    }
    let mut written = 0;
    for buffer in buffers {
        let mut replacements = Vec::new();
        for (from, to, count) in &mappings {
            if from == to || from.is_empty() || from.len() > buffer.len() {
                continue;
            }
            for (offset, bytes) in buffer.windows(from.len()).enumerate() {
                if bytes == from {
                    replacements.push((offset, to, *count));
                }
            }
        }
        for (offset, bytes, count) in replacements {
            buffer[offset..offset + bytes.len()].copy_from_slice(bytes);
            written += count;
        }
    }
    written
}

fn encode_profile(points: &[Vector3]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 + points.len() * POINT_BYTES);
    bytes.extend_from_slice(&(points.len() as u32).to_le_bytes());
    for point in points {
        for value in [point.x, point.y, point.z] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

fn read_f64(data: &[u8], offset: usize) -> Option<f64> {
    let value = f64::from_le_bytes(data.get(offset..offset + 8)?.try_into().ok()?);
    value.is_finite().then_some(value)
}

pub(crate) fn point_is_finite(point: Vector3) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.z.is_finite()
}

pub(crate) fn positive(value: f64) -> bool {
    value.is_finite() && value > f64::EPSILON
}

pub(crate) struct NativeRecord {
    pub preview: Vec<u8>,
    pub preview_bit: i64,
    pub body: DwgMergedReader,
}

/// Decode the actual common header; graphics and the class body need not be byte aligned.
pub(crate) fn native_record_layout(data: &ArchEntityData) -> Option<NativeRecord> {
    if data.dwg_source_version != Some(DxfVersion::AC1032) {
        return None;
    }
    let raw = data.raw_dwg_data.as_deref()?;
    let objects = DwgObjectReader::new(Vec::new(), DxfVersion::AC1032, Default::default()).ok()?;
    let (type_code, mut reader) = objects.read_raw_record(raw, data.dwg_handle_bits).ok()?;
    let mut prefix = reader.clone();
    objects.read_common_data(&mut prefix, type_code);
    if !prefix.read_bit() {
        return None;
    }
    let size = prefix.read_bit_long_long();
    let preview_bit = prefix.position_in_bits();
    let common = objects.read_common_entity_data(&mut reader, type_code);
    reader.check_error().ok()?;
    prefix.check_error().ok()?;
    let preview = common.graphic_data?;
    if size < 0 || usize::try_from(size).ok()? != preview.len() {
        return None;
    }
    ProxyGraphics::decode(&preview)?;
    Some(NativeRecord {
        preview,
        preview_bit,
        body: reader,
    })
}

/// Historical measured offsets start immediately after the cached graphics.
pub(crate) fn native_body_bit(data: &ArchEntityData) -> Option<i64> {
    let record = native_record_layout(data)?;
    Some(record.preview_bit + record.preview.len() as i64 * 8)
}

#[derive(Clone)]
pub(crate) struct NativePlanFrame {
    pub positions: [i64; 12],
    pub values: [f64; 12],
    pub body: DwgMergedReader,
}

/// Decode the shared AecDbEntity placement prefix and leave the reader at its class fields.
pub(crate) fn native_plan_frame(data: &ArchEntityData, schema: i16) -> Option<NativePlanFrame> {
    let mut reader = native_record_layout(data)?.body;
    if reader.read_bit_short() != 80
        || !reader.read_bit()
        || !reader.read_bit()
        || reader.read_bit_short() != schema
        || reader.read_bit_short() != 80
        || !reader.read_variable_text().is_empty()
        || reader.read_bit_short() != 80
        || reader.read_bit_short() != 0
        || reader.read_byte() != 1
        || reader.read_bit()
        || reader.read_bit_short() != 80
    {
        return None;
    }
    // Imported entities carry a placement identifier here; newly created ones may omit it.
    reader.read_variable_text();
    let mut positions = [0; 12];
    let mut values = [0.0_f64; 12];
    for (position, value) in positions.iter_mut().zip(&mut values) {
        *position = reader.position_in_bits();
        *value = reader.read_bit_double();
        if !value.is_finite() {
            return None;
        }
    }
    if !reader.read_variable_text().is_empty() {
        return None;
    }
    reader.read_handle();
    if reader.read_bit_short() != 80 {
        return None;
    }
    reader.check_error().ok()?;
    let along = Vector3::new(values[3], values[4], values[5]);
    let across = Vector3::new(values[6], values[7], values[8]);
    let up = Vector3::new(values[9], values[10], values[11]);
    if along.z.abs() > 1e-9
        || across.z.abs() > 1e-9
        || (along.length() - 1.0).abs() > 1e-6
        || (across - Vector3::new(-along.y, along.x, 0.0)).length() > 1e-6
        || (up - Vector3::UNIT_Z).length() > 1e-6
    {
        return None;
    }
    Some(NativePlanFrame {
        positions,
        values,
        body: reader,
    })
}

/// Parameter positions in the measured 2027 straight-wall layout.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StraightWallFields {
    pub start_x: i64,
    pub start_y: i64,
    pub dir_x: i64,
    pub dir_y: i64,
    pub normal_x: i64,
    pub normal_y: i64,
    pub height: i64,
    pub width: i64,
    pub length: i64,
}

/// Validate the measured frame immediately after the cached graphics.
/// Compact zero/one doubles retain their encoded size when edited.
pub(crate) fn find_straight_wall_fields(data: &ArchEntityData) -> Option<StraightWallFields> {
    let frame = native_plan_frame(data, 569)?;
    if frame.values[2].abs() > 1e-12 {
        return None;
    }
    let mut reader = frame.body;
    reader.read_handle();
    reader.read_handle();
    let height = reader.position_in_bits();
    if !positive(reader.read_bit_double()) || reader.read_bit_double() != -1.0 {
        return None;
    }
    let width = reader.position_in_bits();
    if !positive(reader.read_bit_double()) {
        return None;
    }
    let layout_value = reader.read_bit_double();
    if !matches!(layout_value, 0.0 | 12.0) || reader.read_bit() || reader.read_bit() {
        return None;
    }
    for _ in 0..5 {
        read_empty_group(&mut reader)?;
    }
    if reader.read_bit_short() != 2 {
        return None;
    }
    read_empty_group(&mut reader)?;
    if reader.read_bit_short() != 80 {
        return None;
    }
    let length = reader.position_in_bits();
    if !positive(reader.read_bit_double()) {
        return None;
    }
    reader.check_error().ok()?;
    Some(StraightWallFields {
        start_x: frame.positions[0],
        start_y: frame.positions[1],
        dir_x: frame.positions[3],
        dir_y: frame.positions[4],
        normal_x: frame.positions[6],
        normal_y: frame.positions[7],
        height,
        width,
        length,
    })
}

fn read_empty_group(reader: &mut DwgMergedReader) -> Option<()> {
    (reader.read_bit_short() == 80
        && reader.read_variable_text().is_empty()
        && reader.read_bit_short() == 80
        && reader.read_bit_long() == 0)
        .then_some(())
}

pub(crate) fn write_bytes_at_bit(data: &mut [u8], bit_index: i64, bytes: &[u8]) -> bool {
    let Ok(bit_index) = usize::try_from(bit_index) else {
        return false;
    };
    if bit_index
        .checked_add(bytes.len().saturating_mul(8))
        .is_none_or(|end| end > data.len() * 8)
    {
        return false;
    }
    for (byte_index, byte) in bytes.iter().enumerate() {
        for bit in 0..8 {
            let at = bit_index + byte_index * 8 + bit;
            let mask = 1u8 << (7 - at % 8);
            if byte & (1 << (7 - bit)) != 0 {
                data[at / 8] |= mask
            } else {
                data[at / 8] &= !mask
            }
        }
    }
    true
}

pub(crate) fn read_encoded_double(data: &[u8], bit: i64) -> Option<(f64, i64)> {
    let bit = usize::try_from(bit).ok()?;
    let code = read_bits(data, bit, 2)?;
    match code {
        0 => {
            let mut bytes = [0u8; 8];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = read_bits(data, bit + 2 + index * 8, 8)?;
            }
            let value = f64::from_le_bytes(bytes);
            value.is_finite().then_some((value, bit as i64 + 66))
        }
        1 => Some((1.0, bit as i64 + 2)),
        2 => Some((0.0, bit as i64 + 2)),
        _ => None,
    }
}

fn read_bits(data: &[u8], at: usize, count: usize) -> Option<u8> {
    let mut result = 0;
    for bit in at..at.checked_add(count)? {
        result = (result << 1) | ((data.get(bit / 8)? >> (7 - bit % 8)) & 1);
    }
    Some(result)
}

pub(crate) fn read_bit_double_at(data: &[u8], bit: i64) -> Option<f64> {
    let (value, next) = read_encoded_double(data, bit)?;
    (next - bit == 66).then_some(value)
}

/// Write one bit-coded double.
///
/// A full 66-bit field is overwritten in place. A compact 0 or 1 can switch to
/// the other compact value in place. Any other new value grows that field to
/// 66 bits and returns the 64 bits inserted so later field positions can move
/// with it.
pub(crate) fn write_bit_double_at(data: &mut Vec<u8>, bit: i64, value: f64) -> Option<i64> {
    if !value.is_finite() {
        return None;
    }
    let (old, next) = read_encoded_double(data, bit)?;
    let width = next - bit;
    if width == 66 {
        write_double_at_bit(data, (bit as usize) + 2, value);
        return Some(0);
    }
    if width != 2 {
        return None;
    }
    if (value - old).abs() <= 1e-12 {
        return Some(0);
    }
    if value == 0.0 || value == 1.0 {
        write_bits(
            data,
            bit as usize,
            if value == 0.0 { 0b10 } else { 0b01 },
            2,
        )?;
        return Some(0);
    }
    write_bits(data, bit as usize, 0b00, 2)?;
    insert_bits(data, (bit as usize) + 2, &value.to_le_bytes())?;
    Some(64)
}

/// Apply edits from lowest bit to highest, moving later edits when an earlier
/// compact value has to grow.
pub(crate) fn write_bit_doubles_at(data: &mut Vec<u8>, edits: &[(i64, f64)]) -> bool {
    let mut edits = edits.to_vec();
    edits.sort_by_key(|(bit, _)| *bit);
    let mut shift = 0i64;
    for (bit, value) in edits {
        let Some(grown) = write_bit_double_at(data, bit + shift, value) else {
            return false;
        };
        shift += grown;
    }
    true
}

fn write_bits(data: &mut [u8], at: usize, bits: u8, count: usize) -> Option<()> {
    if count > 8 {
        return None;
    }
    for index in 0..count {
        let pos = at + index;
        let slot = data.get_mut(pos / 8)?;
        let mask = 1u8 << (7 - pos % 8);
        if bits & (1 << (count - 1 - index)) != 0 {
            *slot |= mask;
        } else {
            *slot &= !mask;
        }
    }
    Some(())
}

fn insert_bits(data: &mut Vec<u8>, at: usize, bytes: &[u8]) -> Option<()> {
    let mut bits = Vec::with_capacity(data.len() * 8 + bytes.len() * 8);
    for byte in data.iter() {
        for shift in (0..8).rev() {
            bits.push(byte & (1 << shift) != 0);
        }
    }
    if at > bits.len() {
        return None;
    }
    let mut inserted = Vec::with_capacity(bytes.len() * 8);
    for byte in bytes {
        for shift in (0..8).rev() {
            inserted.push(byte & (1 << shift) != 0);
        }
    }
    bits.splice(at..at, inserted);
    data.clear();
    data.resize(bits.len().div_ceil(8), 0);
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            data[index / 8] |= 1 << (7 - index % 8);
        }
    }
    Some(())
}

pub(crate) fn write_double_at_bit(data: &mut [u8], bit_index: usize, value: f64) {
    for (byte_index, byte) in value.to_le_bytes().into_iter().enumerate() {
        for bit in 0..8 {
            let at = bit_index + byte_index * 8 + bit;
            let Some(slot) = data.get_mut(at / 8) else {
                return;
            };
            let mask = 1u8 << (7 - at % 8);
            if byte & (1 << (7 - bit)) != 0 {
                *slot |= mask
            } else {
                *slot &= !mask
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_profile_replacements_do_not_cascade() {
        let points = vec![Vector3::new(1.0, 2.0, 0.0), Vector3::new(3.0, 4.0, 0.0)];
        let mut bytes = encode_profile(&points);
        assert_eq!(
            map_profile_points(&mut [&mut bytes], &[points], |p| p + Vector3::new(
                2.0, 2.0, 0.0
            )),
            2
        );
        assert_eq!(
            polyline_from_type6(&bytes).unwrap()[0],
            Vector3::new(3.0, 4.0, 0.0)
        );
    }

    #[test]
    fn truncated_and_nonfinite_doubles_are_rejected() {
        assert!(read_bit_double_at(&[0; 8], 0).is_none());
        assert!(write_bit_double_at(&mut vec![0; 16], 0, f64::NAN).is_none());
    }

    #[test]
    fn compact_direction_component_grows_without_moving_the_tail() {
        let tail = 0b1111_1101u8;
        let mut data = vec![0b1000_0000, tail];
        assert_eq!(write_bit_double_at(&mut data, 0, 0.0), Some(0));
        assert_eq!(write_bit_double_at(&mut data, 0, 1.0), Some(0));
        let (one, next) = read_encoded_double(&data, 0).unwrap();
        assert_eq!((one, next), (1.0, 2));
        assert_eq!(write_bit_double_at(&mut data, 0, 12.5), Some(64));
        let (value, next) = read_encoded_double(&data, 0).unwrap();
        assert_eq!(value, 12.5);
        assert_eq!(next, 66);
        assert_eq!(read_bits(&data, 72, 8), Some(tail));
    }
}
