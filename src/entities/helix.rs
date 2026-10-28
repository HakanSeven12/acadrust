//! Helix entity (a 3D spiral curve, AcDbHelix).
//!
//! A helix is defined as a spline (the evaluated curve geometry) plus the
//! parametric fields that generated it: an axis, a start point, a radius, a
//! turn count and a turn height. It derives from the spline entity in the DWG
//! format, so the wire record is the full spline record followed by the helix
//! parameters — this type embeds a [`Spline`] to reuse that geometry directly.

use super::spline::Spline;
use super::{Entity, EntityCommon};
use crate::types::{BoundingBox3D, Vector3};

/// Constraint that is held fixed while the other helix parameters vary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HelixConstraint {
    /// Turn height is held constant (DXF 280 = 0).
    TurnHeight,
    /// Number of turns is held constant (DXF 280 = 1).
    Turns,
    /// Overall height is held constant (DXF 280 = 2).
    Height,
}

impl HelixConstraint {
    /// Decode the DXF 280 constraint code.
    pub fn from_code(code: u8) -> Self {
        match code {
            1 => HelixConstraint::Turns,
            2 => HelixConstraint::Height,
            _ => HelixConstraint::TurnHeight,
        }
    }

    /// Encode to the DXF 280 constraint code.
    pub fn to_code(self) -> u8 {
        match self {
            HelixConstraint::TurnHeight => 0,
            HelixConstraint::Turns => 1,
            HelixConstraint::Height => 2,
        }
    }
}

impl Default for HelixConstraint {
    fn default() -> Self {
        HelixConstraint::TurnHeight
    }
}

/// A helix entity (AcDbHelix): a spline curve plus its generating parameters.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Helix {
    /// Common entity data.
    pub common: EntityCommon,
    /// The evaluated curve geometry (AcDbSpline part of the record).
    pub spline: Spline,
    /// Major release number (DXF 90).
    pub major_version: i32,
    /// Maintenance release number (DXF 91).
    pub maintenance_version: i32,
    /// Axis base point (DXF 10).
    pub axis_base_point: Vector3,
    /// Start point on the first turn (DXF 11).
    pub start_point: Vector3,
    /// Axis direction vector (DXF 12).
    pub axis_vector: Vector3,
    /// Radius at the top/end of the helix (DXF 40 and the DWG helix radius field).
    /// The base radius is defined by the axis base point and start point.
    pub radius: f64,
    /// Number of turns (DXF 41).
    pub turns: f64,
    /// Height of one turn (DXF 42).
    pub turn_height: f64,
    /// Handedness: `false` = left, `true` = right (DXF 290).
    pub handedness: bool,
    /// Which parameter is held constant (DXF 280).
    pub constraint: HelixConstraint,
}

impl Helix {
    /// Create a new, empty helix.
    pub fn new() -> Self {
        Helix {
            common: EntityCommon::new(),
            spline: Spline::new(),
            major_version: 29,
            maintenance_version: 0,
            axis_base_point: Vector3::ZERO,
            start_point: Vector3::ZERO,
            axis_vector: Vector3::UNIT_Z,
            radius: 1.0,
            turns: 3.0,
            turn_height: 1.0,
            handedness: true,
            constraint: HelixConstraint::TurnHeight,
        }
    }
}

impl Default for Helix {
    fn default() -> Self {
        Self::new()
    }
}

impl Entity for Helix {
    fn common(&self) -> &super::EntityCommon {
        &self.common
    }

    fn common_mut(&mut self) -> &mut super::EntityCommon {
        &mut self.common
    }

    fn bounding_box(&self) -> BoundingBox3D {
        self.spline.bounding_box()
    }

    fn translate(&mut self, offset: Vector3) {
        super::translate::translate_helix(self, offset);
    }

    fn entity_type(&self) -> &'static str {
        "HELIX"
    }

    fn apply_transform(&mut self, transform: &crate::types::Transform) {
        super::transform::transform_helix(self, transform);
    }

    fn apply_mirror(&mut self, transform: &crate::types::Transform) {
        super::mirror::mirror_helix(self, transform);
    }
}
