//! Header arrow blocks are names in DXF (`$DIMBLK` = `ArchTick`) and handles in
//! a DWG. The block ACAD creates for that name is `_ArchTick`.

use super::HeaderVariables;
use crate::tables::{BlockRecord, Table};
use crate::types::Handle;

pub(crate) struct ArrowHeader {
    pub names: [String; 4],
    pub handles: [Handle; 4],
}

/// Fill whichever side is missing: a DXF name such as `ArchTick` selects the
/// block `_ArchTick`, and a handle selects the name DXF writes for that block.
pub(crate) fn resolve_dimension_arrow_header(
    blocks: &Table<BlockRecord>,
    header: &HeaderVariables,
) -> ArrowHeader {
    let inputs = [
        (
            header.dim_arrow_block.clone(),
            header.dim_arrow_block_handle,
        ),
        (
            header.dim_arrow_block1.clone(),
            header.dim_arrow_block1_handle,
        ),
        (
            header.dim_arrow_block2.clone(),
            header.dim_arrow_block2_handle,
        ),
        (
            header.dim_leader_arrow_block.clone(),
            header.dim_leader_arrow_block_handle,
        ),
    ];
    let mut resolved = ArrowHeader {
        names: Default::default(),
        handles: [Handle::NULL; 4],
    };
    for (index, (name, handle)) in inputs.into_iter().enumerate() {
        let (name, handle) = resolve_one(blocks, name, handle);
        resolved.names[index] = name;
        resolved.handles[index] = handle;
    }
    resolved
}

pub(crate) fn apply_dimension_arrow_header(
    blocks: &Table<BlockRecord>,
    header: &mut HeaderVariables,
) {
    let resolved = resolve_dimension_arrow_header(blocks, header);
    header.dim_arrow_block = resolved.names[0].clone();
    header.dim_arrow_block1 = resolved.names[1].clone();
    header.dim_arrow_block2 = resolved.names[2].clone();
    header.dim_leader_arrow_block = resolved.names[3].clone();
    header.dim_arrow_block_handle = resolved.handles[0];
    header.dim_arrow_block1_handle = resolved.handles[1];
    header.dim_arrow_block2_handle = resolved.handles[2];
    header.dim_leader_arrow_block_handle = resolved.handles[3];
}

pub(crate) fn sync_dimension_arrow_header(document: &mut super::CadDocument) {
    let resolved = resolve_dimension_arrow_header(&document.block_records, &document.header);
    document.header.dim_arrow_block = resolved.names[0].clone();
    document.header.dim_arrow_block1 = resolved.names[1].clone();
    document.header.dim_arrow_block2 = resolved.names[2].clone();
    document.header.dim_leader_arrow_block = resolved.names[3].clone();
    document.header.dim_arrow_block_handle = resolved.handles[0];
    document.header.dim_arrow_block1_handle = resolved.handles[1];
    document.header.dim_arrow_block2_handle = resolved.handles[2];
    document.header.dim_leader_arrow_block_handle = resolved.handles[3];
}

/// The group-1 value of `$DIMBLK` / `$DIMBLK1` / `$DIMBLK2` / `$DIMLDRBLK`.
pub(crate) fn dimension_arrow_dxf_name(
    blocks: &Table<BlockRecord>,
    stored: &str,
    handle: Handle,
) -> String {
    if !name_is_blank(stored) {
        return stored.trim().to_string();
    }
    blocks
        .iter()
        .find(|record| record.handle == handle)
        .map(|record| dxf_name(&record.name))
        .unwrap_or_default()
}

fn resolve_one(blocks: &Table<BlockRecord>, name: String, mut handle: Handle) -> (String, Handle) {
    if !handle.is_null() && blocks.iter().all(|record| record.handle != handle) {
        handle = Handle::NULL;
    }
    if handle.is_null() {
        handle = handle_for_name(blocks, &name);
    }
    let name = if name_is_blank(&name) {
        if handle.is_null() {
            String::new()
        } else {
            blocks
                .iter()
                .find(|record| record.handle == handle)
                .map(|record| dxf_name(&record.name))
                .unwrap_or_default()
        }
    } else {
        name.trim().to_string()
    };
    (name, handle)
}

fn handle_for_name(blocks: &Table<BlockRecord>, name: &str) -> Handle {
    let trimmed = name.trim();
    if name_is_blank(trimmed) {
        return Handle::NULL;
    }
    if let Some(record) = blocks.get(trimmed) {
        return record.handle;
    }
    if let Some(rest) = trimmed.strip_prefix('_') {
        if let Some(record) = blocks.get(rest) {
            return record.handle;
        }
    } else if let Some(record) = blocks.get(&format!("_{trimmed}")) {
        return record.handle;
    }
    Handle::NULL
}

fn name_is_blank(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty() || trimmed == "."
}

/// ACAD's header stores `ArchTick` for the block `_ArchTick`.
fn dxf_name(block_name: &str) -> String {
    match block_name.strip_prefix('_') {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => block_name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::CadDocument;
    use crate::tables::BlockRecord;

    fn block(doc: &mut CadDocument, name: &str) -> Handle {
        let mut record = BlockRecord::new(name);
        record.handle = doc.allocate_handle();
        record.block_entity_handle = doc.allocate_handle();
        record.block_end_handle = doc.allocate_handle();
        let handle = record.handle;
        doc.block_records.add(record).unwrap();
        handle
    }

    #[test]
    fn an_underscore_block_is_the_header_name_without_it() {
        let mut doc = CadDocument::new();
        let handle = block(&mut doc, "_ArchTick");
        doc.header.dim_arrow_block = "ArchTick".into();
        sync_dimension_arrow_header(&mut doc);
        assert_eq!(doc.header.dim_arrow_block_handle, handle);
        assert_eq!(doc.header.dim_arrow_block, "ArchTick");

        doc.header.dim_arrow_block.clear();
        doc.header.dim_arrow_block_handle = handle;
        sync_dimension_arrow_header(&mut doc);
        assert_eq!(doc.header.dim_arrow_block, "ArchTick");
        assert_eq!(
            dimension_arrow_dxf_name(&doc.block_records, "", handle),
            "ArchTick"
        );
    }
}
