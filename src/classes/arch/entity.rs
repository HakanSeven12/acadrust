//! Editable Arch entity payload.
//!
//! The common entity fields (layer, color, handle) stay on the owning
//! [`crate::entities::ExtendedEntity`]. This struct holds the class identity,
//! the decoded profiles, and the original DWG or DXF body so a save reproduces
//! the record Arch wrote.

use crate::classes::arch::geometry::{
    decode_profiles, find_straight_wall_fields, native_record_layout, point_is_finite, positive,
    read_bit_double_at, read_encoded_double, write_bit_doubles_at,
};
use crate::types::{DxfVersion, Vector3};
use thiserror::Error;

/// Why an architecture edit left the entity unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ArchEditError {
    /// The class, version, or native field layout is not one this edit supports.
    #[error("unsupported architecture layout")]
    UnsupportedLayout,
    /// A size, point, or transform is non-finite, non-positive, or non-planar.
    #[error("invalid architecture value")]
    InvalidValue,
    /// The native body or cached graphics do not match the decoded edit.
    #[error("architecture edit was rejected")]
    Rejected,
}

/// One straight run of a wall: baseline start, baseline end, full thickness, and base height.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StraightWallGeometry {
    pub start: Vector3,
    pub end: Vector3,
    pub width: f64,
    pub height: f64,
}

/// Which Arch entity a class name represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ArchKind {
    Wall,
    Door,
    Window,
    Opening,
    Stair,
    Railing,
    Slab,
    RoofSlab,
    Roof,
    Space,
    CurtainWall,
    CurtainWallUnit,
    WindowAssembly,
    ColumnGrid,
    CeilingGrid,
    Polygon,
    MassElement,
    MassGroup,
    MultiViewBlock,
    MaskBlock,
    Section,
    SectionLine,
    ElevationLine,
    Dimension,
    Schedule,
    LayoutCurve,
    LayoutGrid,
    Zone,
    Area,
    AreaGroup,
    Camera,
    Slice,
    EntityReference,
    Point,
    DisplayTheme,
    StructuralMember,
    /// An Arch entity this crate does not give a narrower name.
    Other,
}

/// Class body of an Arch entity.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ArchEntityData {
    pub dxf_name: String,
    pub cpp_class_name: String,
    pub kind: ArchKind,
    /// Closed or open profiles in world coordinates. The first profile is the
    /// one Arch stored in the proxy-graphics preview.
    pub profiles: Vec<Vec<Vector3>>,
    pub dwg_type_code: i16,
    pub dwg_handle_bits: i64,
    pub dwg_source_version: Option<DxfVersion>,
    pub raw_dwg_data: Option<Vec<u8>>,
    pub raw_dxf_codes: Option<Vec<(i32, String)>>,
    /// Numbers pulled out of [`Self::raw_dwg_data`], filled by [`Self::decode_fields`].
    pub fields: Vec<super::fields::StoredNumber>,
    /// Preview records pulled out of the proxy-graphics blob.
    pub preview: Vec<super::fields::PreviewRecord>,
}

impl ArchEntityData {
    /// Whether this record uses the 2027 straight-wall parameter layout
    /// (start, unit direction, perpendicular, height, width, and length).
    pub fn has_straight_frame(&self) -> bool {
        self.straight_wall_geometry().is_some()
    }

    /// Baseline, thickness, and base height of a straight wall segment.
    pub fn straight_wall_geometry(&self) -> Option<StraightWallGeometry> {
        if self.kind != ArchKind::Wall
            || self.dxf_name != "AEC_WALL"
            || self.dwg_source_version != Some(DxfVersion::AC1032)
        {
            return None;
        }
        let raw = self.raw_dwg_data.as_deref()?;
        let fields = find_straight_wall_fields(self)?;
        let start_x = read_encoded_double(raw, fields.start_x)?.0;
        let start_y = read_encoded_double(raw, fields.start_y)?.0;
        let dir_x = read_encoded_double(raw, fields.dir_x)?.0;
        let dir_y = read_encoded_double(raw, fields.dir_y)?.0;
        let width = read_bit_double_at(raw, fields.width)?;
        let length = read_bit_double_at(raw, fields.length)?;
        let height = read_bit_double_at(raw, fields.height)?;
        let start = Vector3::new(start_x, start_y, 0.0);
        let end = start + Vector3::new(dir_x, dir_y, 0.0) * length;
        Some(StraightWallGeometry {
            start,
            end,
            width,
            height,
        })
    }

    pub fn from_parts(
        dxf_name: impl Into<String>,
        cpp_class_name: impl Into<String>,
        graphic_data: Option<&[u8]>,
    ) -> Self {
        let dxf_name = dxf_name.into();
        let kind = crate::classes::arch::kind_of(&dxf_name).unwrap_or(ArchKind::Other);
        let cpp_class_name = {
            let given = cpp_class_name.into();
            if given.is_empty() {
                crate::classes::arch::cpp_class_name(&dxf_name)
                    .unwrap_or("")
                    .to_string()
            } else {
                given
            }
        };
        Self {
            profiles: decode_profiles(graphic_data),
            dxf_name,
            cpp_class_name,
            kind,
            dwg_type_code: 0,
            dwg_handle_bits: 0,
            dwg_source_version: None,
            raw_dwg_data: None,
            raw_dxf_codes: None,
            fields: Vec::new(),
            preview: super::fields::decode_preview(graphic_data),
        }
    }

    /// Fill [`Self::fields`] from the class bytes. Call this after `raw_dwg_data` is set.
    pub fn decode_fields(&mut self) {
        self.fields = super::fields::decode_numbers(self);
    }

    /// Translate a wall whose native baseline layout is understood.
    /// Unsupported native layouts and non-planar moves leave the entity unchanged.
    pub fn translate(
        &mut self,
        graphic_data: &mut Option<Vec<u8>>,
        delta: Vector3,
    ) -> Result<(), ArchEditError> {
        self.transform_points(graphic_data, |point| point + delta)
    }

    /// Apply a translation to a supported native wall.
    /// Other transforms require a typed setter with a decoded native layout.
    pub fn transform_points(
        &mut self,
        graphic_data: &mut Option<Vec<u8>>,
        map: impl FnMut(Vector3) -> Vector3,
    ) -> Result<(), ArchEditError> {
        let mut map = map;
        let Some(first) = self.profiles.iter().flatten().next().copied() else {
            return Err(ArchEditError::UnsupportedLayout);
        };
        let delta = map(first) - first;
        if !point_is_finite(delta) || delta.z.abs() > 1e-12 {
            return Err(ArchEditError::InvalidValue);
        }
        for point in self.profiles.iter().flatten().copied().skip(1) {
            let mapped = map(point);
            if !point_is_finite(mapped) || (mapped - point - delta).length() > 1e-6 {
                return Err(ArchEditError::Rejected);
            }
        }
        let Some(wall) = self.straight_wall_geometry() else {
            return translate_plan_frame(self, graphic_data, delta);
        };
        set_wall(
            self,
            graphic_data,
            wall.start + delta,
            wall.end + delta,
            wall.width,
            wall.height,
        )
    }

    /// Translate a supported wall using a translated copy of its first profile.
    /// Arbitrary outlines cannot be applied to an opaque native AEC body.
    pub fn reshape(
        &mut self,
        graphic_data: &mut Option<Vec<u8>>,
        points: &[Vector3],
    ) -> Result<(), ArchEditError> {
        let Some(current) = self.profiles.first() else {
            return Err(ArchEditError::UnsupportedLayout);
        };
        if current.len() != points.len() || points.is_empty() {
            return Err(ArchEditError::Rejected);
        }
        let pairs: Vec<(Vector3, Vector3)> = current
            .iter()
            .copied()
            .zip(points.iter().copied())
            .collect();
        self.transform_points(graphic_data, |point| {
            pairs
                .iter()
                .find(|(from, _)| *from == point)
                .map(|(_, to)| *to)
                .unwrap_or(point)
        })
    }
}

/// Move a straight wall whose placement frame is known but whose class tail is not
/// the center-justified segment layout `set_wall` edits.
fn translate_plan_frame(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    delta: Vector3,
) -> Result<(), ArchEditError> {
    let Some(frame) = super::geometry::native_plan_frame(data, 569) else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    if frame.values[2].abs() > 1e-9 {
        return Err(ArchEditError::Rejected);
    }
    let x = frame.values[0] + delta.x;
    let y = frame.values[1] + delta.y;
    if !x.is_finite() || !y.is_finite() {
        return Err(ArchEditError::InvalidValue);
    }
    transaction(data, graphic_data, |next, graphics| {
        let Some(raw) = next.raw_dwg_data.as_mut() else {
            return Err(ArchEditError::UnsupportedLayout);
        };
        if !super::geometry::write_bit_doubles_at(
            raw,
            &[(frame.positions[0], x), (frame.positions[1], y)],
        ) {
            return Err(ArchEditError::Rejected);
        }
        rewrite_preview(next, graphics, |point| point + delta)
    })
}

pub(crate) fn transaction(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    edit: impl FnOnce(&mut ArchEntityData, &mut Option<Vec<u8>>) -> Result<(), ArchEditError>,
) -> Result<(), ArchEditError> {
    let mut next = data.clone();
    let mut graphics = graphic_data.clone();
    edit(&mut next, &mut graphics)?;
    next.decode_fields();
    *data = next;
    *graphic_data = graphics;
    Ok(())
}

/// Update only parsed type-6 records in the complete, validated cached metafile.
/// Callers must also update the corresponding native parameters transactionally.
pub(crate) fn rewrite_preview(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    mut map: impl FnMut(Vector3) -> Vector3,
) -> Result<(), ArchEditError> {
    use crate::entities::{ProxyGraphicRecord, ProxyGraphics};
    let Some(record) = native_record_layout(data) else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    let original = record.preview.as_slice();
    if graphic_data
        .as_deref()
        .is_some_and(|graphics| graphics != original)
        || decode_profiles(Some(original)) != data.profiles
    {
        return Err(ArchEditError::Rejected);
    }
    let Some(mut graphics) = ProxyGraphics::decode(original) else {
        return Err(ArchEditError::Rejected);
    };
    for record in &mut graphics.records {
        let ProxyGraphicRecord::Unknown {
            record_type: 6,
            data: payload,
        } = record
        else {
            continue;
        };
        let Some(points) = super::geometry::polyline_from_type6(payload) else {
            return Err(ArchEditError::Rejected);
        };
        for (index, point) in points.into_iter().enumerate() {
            let point = map(point);
            if !point_is_finite(point) {
                return Err(ArchEditError::InvalidValue);
            }
            let offset = 4 + index * 24;
            payload[offset..offset + 8].copy_from_slice(&point.x.to_le_bytes());
            payload[offset + 8..offset + 16].copy_from_slice(&point.y.to_le_bytes());
            payload[offset + 16..offset + 24].copy_from_slice(&point.z.to_le_bytes());
        }
    }
    let Some(encoded) = graphics.encode() else {
        return Err(ArchEditError::Rejected);
    };
    if encoded.len() != original.len() {
        return Err(ArchEditError::Rejected);
    }
    let Some(raw) = data.raw_dwg_data.as_mut() else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    if !super::geometry::write_bytes_at_bit(raw, record.preview_bit, &encoded) {
        return Err(ArchEditError::Rejected);
    }
    data.profiles = decode_profiles(Some(&encoded));
    data.preview = super::fields::decode_preview(Some(&encoded));
    if graphic_data.is_some() {
        *graphic_data = Some(encoded)
    }
    Ok(())
}

/// Build a rectangular wall footprint from a baseline and a width.
///
/// The rectangle is closed (the first point is repeated). `width` is applied
/// to the left of the baseline direction.
pub fn rectangle_profile(start: Vector3, end: Vector3, width: f64) -> Option<Vec<Vector3>> {
    let edge = end - start;
    let length = edge.length();
    if !point_is_finite(start)
        || !point_is_finite(end)
        || !positive(length)
        || !positive(width)
        || (start.z - end.z).abs() > 1e-12
    {
        return None;
    }
    let normal = Vector3::new(-edge.y / length, edge.x / length, 0.0) * width;
    Some(vec![start, end, end + normal, start + normal, start])
}

/// Rewrite one straight wall onto a new baseline.
///
/// A straight wall is one run from `start` to `end` at any angle in the XY
/// plane, including a horizontal run. `width` is the full thickness and
/// `height` is the base height. The preview becomes the center-justified
/// rectangle around that baseline. A compact 0 or 1 direction component grows
/// in place when the new direction needs a full number.
pub fn set_wall(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    start: Vector3,
    end: Vector3,
    width: f64,
    height: f64,
) -> Result<(), ArchEditError> {
    if data.kind != ArchKind::Wall || data.straight_wall_geometry().is_none() {
        return Err(ArchEditError::UnsupportedLayout);
    }
    let Some(current) = data.profiles.first() else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    if data.profiles.len() != 1 || current.len() != 5 || current.first() != current.last() {
        return Err(ArchEditError::Rejected);
    }
    let edge = end - start;
    let length = edge.length();
    if !point_is_finite(start)
        || !point_is_finite(end)
        || !positive(length)
        || !positive(width)
        || !positive(height)
        || start.z.abs() > 1e-12
        || end.z.abs() > 1e-12
    {
        return Err(ArchEditError::InvalidValue);
    }
    let Some(fields) = find_straight_wall_fields(data) else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    let Some(points) = centered_wall_profile(start, end, width) else {
        return Err(ArchEditError::InvalidValue);
    };
    let along = edge * (1.0 / length);
    let current = current.clone();
    transaction(data, graphic_data, |next, graphics| {
        let Some(raw) = next.raw_dwg_data.as_mut() else {
            return Err(ArchEditError::UnsupportedLayout);
        };
        if !write_bit_doubles_at(
            raw,
            &[
                (fields.start_x, start.x),
                (fields.start_y, start.y),
                (fields.dir_x, along.x),
                (fields.dir_y, along.y),
                (fields.normal_x, -along.y),
                (fields.normal_y, along.x),
                (fields.height, height),
                (fields.width, width),
                (fields.length, length),
            ],
        ) {
            return Err(ArchEditError::Rejected);
        }
        rewrite_preview(next, graphics, |point| {
            current
                .iter()
                .position(|old| *old == point)
                .map(|index| points[index])
                .unwrap_or(point)
        })
    })
}

/// Resize the bounds of an axis-aligned wall with a decoded native baseline.
/// Native geometry for other entity kinds must be edited with a typed setter.
pub fn set_profile_bounds(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    min: Vector3,
    max: Vector3,
) -> Result<(), ArchEditError> {
    if !point_is_finite(min)
        || !point_is_finite(max)
        || !(max.x > min.x)
        || !(max.y > min.y)
        || min.z.abs() > 1e-12
        || max.z.abs() > 1e-12
    {
        return Err(ArchEditError::InvalidValue);
    }
    let Some(wall) = data.straight_wall_geometry() else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    let direction = wall.end - wall.start;
    let (mut start, mut end, width) = if direction.x.abs() < 1e-6 {
        let x = (min.x + max.x) * 0.5;
        (
            Vector3::new(x, min.y, 0.0),
            Vector3::new(x, max.y, 0.0),
            max.x - min.x,
        )
    } else if direction.y.abs() < 1e-6 {
        let y = (min.y + max.y) * 0.5;
        (
            Vector3::new(min.x, y, 0.0),
            Vector3::new(max.x, y, 0.0),
            max.y - min.y,
        )
    } else {
        return Err(ArchEditError::Rejected);
    };
    if (end - start).dot(&direction) < 0.0 {
        std::mem::swap(&mut start, &mut end)
    }
    set_wall(data, graphic_data, start, end, width, wall.height)
}

/// Closed preview of a center-justified straight wall.
///
/// The rectangle follows the baseline at any angle. It starts on the right of
/// the direction, goes to the end, and returns along the left side.
pub fn centered_wall_profile(start: Vector3, end: Vector3, width: f64) -> Option<Vec<Vector3>> {
    let edge = end - start;
    let length = edge.length();
    if !point_is_finite(start)
        || !point_is_finite(end)
        || !positive(length)
        || !positive(width)
        || (start.z - end.z).abs() > 1e-12
    {
        return None;
    }
    let along = edge * (1.0 / length);
    let left = Vector3::new(-along.y, along.x, 0.0) * (width * 0.5);
    Some(vec![
        start - left,
        end - left,
        end + left,
        start + left,
        start - left,
    ])
}

/// Reshape the first profile to a rectangle when it has the same point count.
pub fn reshape_rectangle(
    data: &mut ArchEntityData,
    graphic_data: &mut Option<Vec<u8>>,
    start: Vector3,
    end: Vector3,
    width: f64,
) -> Result<(), ArchEditError> {
    let Some(wall) = data.straight_wall_geometry() else {
        return Err(ArchEditError::UnsupportedLayout);
    };
    let Some(points) = rectangle_profile(start, end, width) else {
        return Err(ArchEditError::InvalidValue);
    };
    let offset = (points[3] - points[0]) * 0.5;
    set_wall(
        data,
        graphic_data,
        start + offset,
        end + offset,
        width,
        wall.height,
    )
}
