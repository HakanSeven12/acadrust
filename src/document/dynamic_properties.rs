//! Read-only projection of dynamic definition parameters and saved insertion state.
//! This does not evaluate actions or mutate geometry.
use super::CadDocument;
use crate::{
    objects::*,
    types::{DxfVersion, Vector3},
    xdata::XDataValue,
    EntityType, Handle,
};

#[derive(Debug, Clone, PartialEq)]
pub struct DynamicBlockProperty {
    pub parameter_handle: Handle,
    pub name: String,
    pub description: String,
    pub type_code: i16,
    /// 0: unitless, 1: angle (radians), 2: distance, 3: area.
    pub units: i16,
    pub show: bool,
    pub read_only: bool,
    pub visible: bool,
    pub value: BlockEvalValue,
    pub allowed_values: Vec<BlockEvalValue>,
}

impl CadDocument {
    fn dynamic_dictionary_entry(&self, dictionary: Handle, key: &str) -> Option<Handle> {
        let ObjectType::Dictionary(d) = self.objects.get(&dictionary)? else {
            return None;
        };
        d.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, h)| *h)
    }

    fn block_graph(&self, definition: Handle) -> Option<&BlockEvaluationGraph> {
        let dict = if self.dwg_source_version.is_some() {
            *self.xdic_by_handle.get(&definition)?
        } else {
            self.extension_dictionary_handle(definition)?
        };
        let graph = self.dynamic_dictionary_entry(dict, "ACAD_ENHANCEDBLOCK")?;
        match self.objects.get(&graph)? {
            ObjectType::DynamicBlock(DynamicBlockObject {
                data: DynamicBlockData::EvaluationGraph(graph),
                ..
            }) => Some(graph),
            _ => None,
        }
    }

    pub fn is_dynamic_block(&self, definition: Handle) -> bool {
        self.block_graph(definition).is_some()
    }

    /// Resolve a saved anonymous representation, including variants with no INSERTs.
    pub fn dynamic_block_definition(&self, block: Handle) -> Option<Handle> {
        if self.is_dynamic_block(block) {
            return Some(block);
        }
        if let Some(records) = self.eed_by_handle.get(&block) {
            let wide = self.dwg_source_version.unwrap_or(self.version) >= DxfVersion::AC1021;
            for (app, payload) in records {
                if !self
                    .app_ids
                    .iter()
                    .any(|a| a.handle.value() == *app && a.name == "AcDbBlockRepBTag")
                {
                    continue;
                }
                let values = crate::io::dwg::eed_codec::decode_values(payload, wide, |_| None)?;
                for value in values {
                    if let XDataValue::Handle(handle) = value {
                        if self.is_dynamic_block(handle) {
                            return Some(handle);
                        }
                    }
                }
            }
        }
        if self.dwg_source_version.is_some() {
            return None;
        }
        // Also supports programmatically constructed documents and DXF imports.
        self.entities().find_map(|e| match e {
            EntityType::Insert(i)
                if self
                    .block_records
                    .get(&i.block_name)
                    .is_some_and(|b| b.handle == block) =>
            {
                self.dynamic_definition_for_insert(i.common.handle)
                    .filter(|h| *h != block)
            }
            _ => None,
        })
    }

    fn saved_parameter(&self, insert: Handle, node: i32) -> Option<&XRecord> {
        let mut dict = self.extension_dictionary_handle(insert)?;
        for key in [
            "AcDbBlockRepresentation",
            "AppDataCache",
            "ACAD_ENHANCEDBLOCKDATA",
        ] {
            dict = self.dynamic_dictionary_entry(dict, key)?;
        }
        let record = self.dynamic_dictionary_entry(dict, &node.to_string())?;
        match self.objects.get(&record)? {
            ObjectType::XRecord(r) => Some(r),
            _ => None,
        }
    }

    /// Public scalar properties. `None` reads definition defaults, never a sample
    /// insertion. `Some(insert)` overlays the saved evaluation cache for that INSERT.
    pub fn dynamic_block_properties(
        &self,
        definition: Handle,
        insert: Option<Handle>,
    ) -> Result<Vec<DynamicBlockProperty>, crate::error::DxfError> {
        let scale = match insert.and_then(|h| self.get_entity(h)) {
            Some(EntityType::Insert(i)) => {
                let s = i.x_scale().abs();
                // ACAD exposes no dynamic property collection for a
                // nonuniformly scaled reference. Mirroring is still uniform.
                if (i.y_scale().abs() - s).abs() > 1e-10 * s.max(1.0)
                    || (i.z_scale().abs() - s).abs() > 1e-10 * s.max(1.0)
                {
                    return Ok(Vec::new());
                }
                s
            }
            _ => 1.0,
        };
        let Some(graph) = self.block_graph(definition) else {
            return Ok(Vec::new());
        };
        let mut result = Vec::new();
        for node in &graph.nodes {
            let Some(object) = self.objects.get(&node.expression) else {
                return Err(format!(
                    "Missing dynamic evaluation node {:X}",
                    node.expression.value()
                )
                .into());
            };
            let data = match object {
                ObjectType::DynamicBlock(o) => &o.data,
                ObjectType::BlockVisibilityParameter(v) => {
                    self.visibility_property(v, insert, &mut result)?;
                    continue;
                }
                _ => {
                    return Err(format!(
                        "Unsupported dynamic evaluation object {:X}",
                        node.expression.value()
                    )
                    .into())
                }
            };
            let mut add = |parameter: &BlockParameter,
                           name: &str,
                           description: &str,
                           units,
                           value,
                           allowed: Vec<_>| {
                result.push(DynamicBlockProperty {
                    parameter_handle: node.expression,
                    name: name.into(),
                    description: description.into(),
                    type_code: match value {
                        BlockEvalValue::Text(_) => 5,
                        BlockEvalValue::Short(_) => 3,
                        BlockEvalValue::Long(_) => 2,
                        _ => 1,
                    },
                    units,
                    show: parameter.show_properties,
                    read_only: false,
                    visible: true,
                    value,
                    allowed_values: allowed,
                });
            };
            let saved = |id| -> Result<Option<&XRecord>, crate::error::DxfError> {
                let record = insert.and_then(|i| self.saved_parameter(i, id));
                if record.is_some_and(|r| !r.entries_complete) {
                    return Err("Incomplete dynamic parameter cache".into());
                }
                if let Some(record) = record {
                    let valid = match data {
                        DynamicBlockData::FlipParameter(_) => cache_flip(record).is_some(),
                        DynamicBlockData::LookupParameter(_) => cache_text(record).is_some(),
                        DynamicBlockData::PointParameter(_) => !cache_points(record).is_empty(),
                        DynamicBlockData::RotationParameter(_) => cache_points(record).len() >= 4,
                        _ => cache_points(record).len() >= 2,
                    };
                    if !valid {
                        return Err(
                            format!("Malformed dynamic parameter cache for node {id}").into()
                        );
                    }
                }
                Ok(record)
            };
            match data {
                DynamicBlockData::VisibilityParameter(v) => {
                    self.visibility_property(v, insert, &mut result)?
                }
                DynamicBlockData::LinearParameter(p) => {
                    let (a, b) = points(
                        &p.parameter,
                        saved(p.parameter.parameter.element.eval.node_id)?,
                    );
                    add(
                        &p.parameter.parameter,
                        &p.distance_name,
                        &p.distance_description,
                        2,
                        BlockEvalValue::Real((b - a).length()),
                        choices(&p.value_set),
                    );
                }
                DynamicBlockData::PolarParameter(p) => {
                    let (a, b) = points(
                        &p.parameter,
                        saved(p.parameter.parameter.element.eval.node_id)?,
                    );
                    add(
                        &p.parameter.parameter,
                        &p.distance_name,
                        &p.distance_description,
                        2,
                        BlockEvalValue::Real((b - a).length()),
                        choices(&p.distance_value_set),
                    );
                    add(
                        &p.parameter.parameter,
                        &p.angle_name,
                        &p.angle_description,
                        1,
                        BlockEvalValue::Real(angle(b - a)),
                        choices(&p.angle_value_set),
                    );
                }
                DynamicBlockData::RotationParameter(p) => {
                    let record = saved(p.parameter.parameter.element.eval.node_id)?;
                    let (a, b) = points(&p.parameter, record);
                    let base = record
                        .and_then(|r| cache_points(r).get(3).copied())
                        .unwrap_or(p.definition_base_angle_point);
                    let mut value = angle(b - a) - angle(base - a);
                    // A flip action can reflect the parameter itself. The cache
                    // holds reflected geometry but the exposed angle retains
                    // the parameter's original orientation.
                    if let Some(insert) = insert {
                        for action in graph.nodes.iter().filter_map(|n| {
                            match self.objects.get(&n.expression) {
                                Some(ObjectType::DynamicBlock(DynamicBlockObject {
                                    data: DynamicBlockData::FlipAction(a),
                                    ..
                                })) if a.action.dependencies.contains(&node.expression) => Some(a),
                                _ => None,
                            }
                        }) {
                            let flip = self
                                .saved_parameter(insert, action.connections[0].code)
                                .and_then(cache_flip)
                                .unwrap_or(0);
                            if flip == 1 {
                                value = -value;
                            }
                        }
                    }
                    let value = value.rem_euclid(std::f64::consts::TAU);
                    add(
                        &p.parameter.parameter,
                        &p.angle_name,
                        &p.angle_description,
                        1,
                        BlockEvalValue::Real(value),
                        choices(&p.value_set),
                    );
                }
                DynamicBlockData::XYParameter(p) => {
                    let (a, b) = points(
                        &p.parameter,
                        saved(p.parameter.parameter.element.eval.node_id)?,
                    );
                    add(
                        &p.parameter.parameter,
                        &p.x_label,
                        &p.x_label_description,
                        2,
                        BlockEvalValue::Real((b.x - a.x).abs()),
                        choices(&p.x_value_set),
                    );
                    add(
                        &p.parameter.parameter,
                        &p.y_label,
                        &p.y_label_description,
                        2,
                        BlockEvalValue::Real((b.y - a.y).abs()),
                        choices(&p.y_value_set),
                    );
                }
                DynamicBlockData::PointParameter(p) => {
                    let record = saved(p.parameter.parameter.element.eval.node_id)?;
                    let point = record
                        .and_then(|r| cache_points(r).first().copied())
                        .unwrap_or(p.parameter.definition_point);
                    add(
                        &p.parameter.parameter,
                        &format!("{} X", p.position_name),
                        &p.position_description,
                        2,
                        BlockEvalValue::Real(point.x),
                        vec![],
                    );
                    add(
                        &p.parameter.parameter,
                        &format!("{} Y", p.position_name),
                        &p.position_description,
                        2,
                        BlockEvalValue::Real(point.y),
                        vec![],
                    );
                }
                DynamicBlockData::FlipParameter(p) => {
                    let record = saved(p.parameter.parameter.element.eval.node_id)?;
                    let value = record.and_then(cache_flip).unwrap_or(0);
                    add(
                        &p.parameter.parameter,
                        &p.flip_label,
                        &p.flip_label_description,
                        0,
                        BlockEvalValue::Short(value),
                        vec![BlockEvalValue::Short(0), BlockEvalValue::Short(1)],
                    );
                }
                DynamicBlockData::LookupParameter(p) => {
                    let action = graph
                        .nodes
                        .iter()
                        .filter_map(|n| match self.objects.get(&n.expression) {
                            Some(ObjectType::DynamicBlock(DynamicBlockObject {
                                data: DynamicBlockData::LookupAction(a),
                                ..
                            })) if a.action.element.eval.node_id == p.index => Some(a),
                            _ => None,
                        })
                        .next()
                        .ok_or_else(|| format!("Missing lookup action for {}", p.lookup_name))?;
                    let id = p.parameter.parameter.element.eval.node_id;
                    let (column, descriptor) = action
                        .columns
                        .iter()
                        .enumerate()
                        .find(|(_, c)| c.node_id == id)
                        .ok_or_else(|| format!("Missing lookup column for {}", p.lookup_name))?;
                    let mut allowed = Vec::new();
                    let width = usize::try_from(action.column_count)
                        .map_err(|_| "Invalid lookup column count")?;
                    if width == 0 || column >= width {
                        return Err("Invalid lookup table dimensions".into());
                    }
                    if action.row_count < 0
                        || action.expressions.len()
                            != width.saturating_mul(action.row_count as usize)
                        || action.columns.len() != width
                    {
                        return Err("Incomplete lookup table".into());
                    }
                    for row in action.expressions.chunks_exact(width) {
                        let value = BlockEvalValue::Text(row[column].clone());
                        if !allowed.contains(&value) {
                            allowed.push(value);
                        }
                    }
                    let unmatched = descriptor.unmatched_name.clone();
                    // A table containing only lookup outputs is a simple choice
                    // list. The unmatched state exists only with input columns.
                    let has_inputs = action.columns.iter().any(|c| !c.lookup_property);
                    let fallback = if has_inputs {
                        BlockEvalValue::Text(unmatched)
                    } else {
                        allowed
                            .first()
                            .cloned()
                            .unwrap_or(BlockEvalValue::Text(String::new()))
                    };
                    let current = saved(id)?
                        .and_then(cache_text)
                        .map(BlockEvalValue::Text)
                        .unwrap_or_else(|| fallback.clone());
                    if has_inputs && current == fallback && !allowed.contains(&fallback) {
                        allowed.push(fallback);
                    }
                    add(
                        &p.parameter.parameter,
                        &p.lookup_name,
                        &p.lookup_description,
                        0,
                        current,
                        allowed,
                    );
                    result.last_mut().unwrap().read_only = !descriptor.writable;
                }
                DynamicBlockData::UserParameter(p) => add(
                    &p.parameter.parameter,
                    &p.parameter.parameter.element.name,
                    "",
                    p.value_type,
                    p.value.clone(),
                    vec![],
                ),
                DynamicBlockData::Unknown => {
                    return Err("Unsupported dynamic block property table".into())
                }
                // These graph nodes do not expose scalar reference properties.
                DynamicBlockData::AlignmentParameter(_)
                | DynamicBlockData::BasePointParameter(_) => {}
                DynamicBlockData::AlignedConstraintParameter(p)
                | DynamicBlockData::LinearConstraintParameter(p)
                | DynamicBlockData::HorizontalConstraintParameter(p)
                | DynamicBlockData::VerticalConstraintParameter(p) => {
                    let base = &p.constraint.parameter;
                    let (a, b) = points(base, saved(base.parameter.element.eval.node_id)?);
                    let value = match data {
                        DynamicBlockData::HorizontalConstraintParameter(_) => (b.x - a.x).abs(),
                        DynamicBlockData::VerticalConstraintParameter(_) => (b.y - a.y).abs(),
                        _ => (b - a).length(),
                    };
                    add(
                        &base.parameter,
                        &p.expression_name,
                        &p.expression_description,
                        2,
                        BlockEvalValue::Real(value),
                        choices(&p.value_set),
                    );
                }
                DynamicBlockData::AngularConstraintParameter(_)
                | DynamicBlockData::DiametricConstraintParameter(_)
                | DynamicBlockData::RadialConstraintParameter(_) => {
                    return Err("Unsupported dynamic constraint property".into())
                }
                _ => {}
            }
        }
        if let Some(v) = self.block_visibility_param_for_def(definition) {
            let selected = result
                .iter()
                .find(|p| p.parameter_handle == v.handle)
                .and_then(|p| match &p.value {
                    BlockEvalValue::Text(s) => Some(s.as_str()),
                    _ => None,
                });
            if let Some(state) = selected.and_then(|s| v.states.iter().find(|v| v.name == s)) {
                for p in &mut result {
                    p.visible = p.parameter_handle == v.handle
                        || state.visible_params.contains(&p.parameter_handle);
                }
            }
        }
        for property in &mut result {
            let multiplier = match property.units {
                2 => scale,
                3 => scale * scale,
                _ => 1.0,
            };
            for value in
                std::iter::once(&mut property.value).chain(property.allowed_values.iter_mut())
            {
                if let BlockEvalValue::Real(v) = value {
                    *v *= multiplier;
                }
            }
        }
        Ok(result)
    }

    fn visibility_property(
        &self,
        p: &BlockVisibilityParameter,
        insert: Option<Handle>,
        result: &mut Vec<DynamicBlockProperty>,
    ) -> Result<(), crate::error::DxfError> {
        let record = insert.and_then(|i| self.saved_parameter(i, p.eval_node_id));
        if record.is_some_and(|r| !r.entries_complete) {
            return Err("Incomplete visibility cache".into());
        }
        if record.is_some_and(|r| cache_text(r).is_none()) {
            return Err("Malformed visibility cache".into());
        }
        let default = p.states.first().map(|s| s.name.clone()).unwrap_or_default();
        result.push(DynamicBlockProperty {
            parameter_handle: p.handle,
            name: p.name.clone(),
            description: p.description.clone(),
            type_code: 5,
            units: 0,
            show: p.show_properties,
            read_only: false,
            visible: true,
            value: BlockEvalValue::Text(record.and_then(cache_text).unwrap_or(default)),
            allowed_values: p
                .states
                .iter()
                .map(|s| BlockEvalValue::Text(s.name.clone()))
                .collect(),
        });
        Ok(())
    }
}

fn cache_points(record: &XRecord) -> Vec<Vector3> {
    record
        .entries
        .iter()
        .filter_map(|e| match e.value {
            XRecordValue::Point3D(x, y, z) => Some(Vector3::new(x, y, z)),
            _ => None,
        })
        .collect()
}
fn cache_text(record: &XRecord) -> Option<String> {
    record.entries.iter().find_map(|e| match &e.value {
        XRecordValue::String(v) if e.code == 1 => Some(v.clone()),
        _ => None,
    })
}
fn cache_flip(record: &XRecord) -> Option<i16> {
    record
        .entries
        .iter()
        .skip(4)
        .rev()
        .find_map(|e| match e.value {
            XRecordValue::Int16(v) => Some(v),
            _ => None,
        })
}
fn points(p: &BlockTwoPointParameter, record: Option<&XRecord>) -> (Vector3, Vector3) {
    let points = record.map(cache_points).unwrap_or_default();
    (
        points.first().copied().unwrap_or(p.definition_base_point),
        points.get(1).copied().unwrap_or(p.definition_end_point),
    )
}
fn angle(v: Vector3) -> f64 {
    v.y.atan2(v.x).rem_euclid(std::f64::consts::TAU)
}
fn choices(set: &BlockParameterValueSet) -> Vec<BlockEvalValue> {
    set.values
        .iter()
        .copied()
        .map(BlockEvalValue::Real)
        .collect()
}
