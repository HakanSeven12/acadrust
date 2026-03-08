/// Diagnostic: dump handle map, header handles, and check for nulls
/// in the written DWG structure.
use acadrust::document::CadDocument;
use acadrust::types::{DxfVersion, Handle};

fn main() -> acadrust::Result<()> {
    // ── Test 1: Fresh document ──
    println!("=== Test 1: Fresh CadDocument ===");
    let doc = CadDocument::new();
    diagnose(&doc, "fresh");

    // ── Test 2: Roundtripped document ──
    println!("\n=== Test 2: Roundtripped from Çizim1.dwg ===");
    let mut reader = acadrust::io::dwg::DwgReader::from_file("Çizim1.dwg")?;
    let doc2 = reader.read()?;
    diagnose(&doc2, "roundtrip");

    Ok(())
}

fn diagnose(doc: &CadDocument, label: &str) {
    let h = &doc.header;

    println!("\n-- Header handle references ({label}) --");
    println!("  block_control_handle:       {:#06X}", h.block_control_handle.value());
    println!("  layer_control_handle:       {:#06X}", h.layer_control_handle.value());
    println!("  style_control_handle:       {:#06X}", h.style_control_handle.value());
    println!("  linetype_control_handle:    {:#06X}", h.linetype_control_handle.value());
    println!("  view_control_handle:        {:#06X}", h.view_control_handle.value());
    println!("  ucs_control_handle:         {:#06X}", h.ucs_control_handle.value());
    println!("  vport_control_handle:       {:#06X}", h.vport_control_handle.value());
    println!("  appid_control_handle:       {:#06X}", h.appid_control_handle.value());
    println!("  dimstyle_control_handle:    {:#06X}", h.dimstyle_control_handle.value());
    println!("  vpent_hdr_control_handle:   {:#06X}", h.vpent_hdr_control_handle.value());
    println!("  named_objects_dict_handle:  {:#06X}", h.named_objects_dict_handle.value());
    println!("  model_space_block_handle:   {:#06X}", h.model_space_block_handle.value());
    println!("  paper_space_block_handle:   {:#06X}", h.paper_space_block_handle.value());
    println!("  bylayer_linetype_handle:    {:#06X}", h.bylayer_linetype_handle.value());
    println!("  byblock_linetype_handle:    {:#06X}", h.byblock_linetype_handle.value());
    println!("  continuous_linetype_handle: {:#06X}", h.continuous_linetype_handle.value());
    println!("  current_layer_handle:       {:#06X}", h.current_layer_handle.value());
    println!("  current_text_style_handle:  {:#06X}", h.current_text_style_handle.value());
    println!("  current_linetype_handle:    {:#06X}", h.current_linetype_handle.value());
    println!("  current_dimstyle_handle:    {:#06X}", h.current_dimstyle_handle.value());
    println!("  acad_group_dict_handle:     {:#06X}", h.acad_group_dict_handle.value());
    println!("  acad_mlinestyle_dict_handle:{:#06X}", h.acad_mlinestyle_dict_handle.value());
    println!("  acad_layout_dict_handle:    {:#06X}", h.acad_layout_dict_handle.value());
    println!("  acad_plotsettings_dict:     {:#06X}", h.acad_plotsettings_dict_handle.value());
    println!("  acad_plotstylename_dict:    {:#06X}", h.acad_plotstylename_dict_handle.value());
    println!("  acad_material_dict:         {:#06X}", h.acad_material_dict_handle.value());
    println!("  acad_color_dict:            {:#06X}", h.acad_color_dict_handle.value());
    println!("  acad_visualstyle_dict:      {:#06X}", h.acad_visualstyle_dict_handle.value());
    println!("  handle_seed:                {:#06X}", h.handle_seed);

    println!("\n-- Table handles vs header ({label}) --");
    println!("  block_records.handle():     {:#06X}  header: {:#06X}  {}",
        doc.block_records.handle().value(),
        h.block_control_handle.value(),
        if doc.block_records.handle() == h.block_control_handle { "OK" } else { "MISMATCH!" });
    println!("  layers.handle():            {:#06X}  header: {:#06X}  {}",
        doc.layers.handle().value(),
        h.layer_control_handle.value(),
        if doc.layers.handle() == h.layer_control_handle { "OK" } else { "MISMATCH!" });
    println!("  text_styles.handle():       {:#06X}  header: {:#06X}  {}",
        doc.text_styles.handle().value(),
        h.style_control_handle.value(),
        if doc.text_styles.handle() == h.style_control_handle { "OK" } else { "MISMATCH!" });
    println!("  line_types.handle():        {:#06X}  header: {:#06X}  {}",
        doc.line_types.handle().value(),
        h.linetype_control_handle.value(),
        if doc.line_types.handle() == h.linetype_control_handle { "OK" } else { "MISMATCH!" });
    println!("  views.handle():             {:#06X}  header: {:#06X}  {}",
        doc.views.handle().value(),
        h.view_control_handle.value(),
        if doc.views.handle() == h.view_control_handle { "OK" } else { "MISMATCH!" });
    println!("  ucss.handle():              {:#06X}  header: {:#06X}  {}",
        doc.ucss.handle().value(),
        h.ucs_control_handle.value(),
        if doc.ucss.handle() == h.ucs_control_handle { "OK" } else { "MISMATCH!" });
    println!("  vports.handle():            {:#06X}  header: {:#06X}  {}",
        doc.vports.handle().value(),
        h.vport_control_handle.value(),
        if doc.vports.handle() == h.vport_control_handle { "OK" } else { "MISMATCH!" });
    println!("  app_ids.handle():           {:#06X}  header: {:#06X}  {}",
        doc.app_ids.handle().value(),
        h.appid_control_handle.value(),
        if doc.app_ids.handle() == h.appid_control_handle { "OK" } else { "MISMATCH!" });
    println!("  dim_styles.handle():        {:#06X}  header: {:#06X}  {}",
        doc.dim_styles.handle().value(),
        h.dimstyle_control_handle.value(),
        if doc.dim_styles.handle() == h.dimstyle_control_handle { "OK" } else { "MISMATCH!" });

    // Check model/paper space block record handles
    if let Some(ms) = doc.block_records.get("*Model_Space") {
        println!("\n  *Model_Space BR handle:     {:#06X}  header: {:#06X}  {}",
            ms.handle.value(),
            h.model_space_block_handle.value(),
            if ms.handle == h.model_space_block_handle { "OK" } else { "MISMATCH!" });
        println!("    block_entity_handle:      {:#06X}", ms.block_entity_handle.value());
        println!("    block_end_handle:         {:#06X}", ms.block_end_handle.value());
        println!("    layout:                   {:#06X}", ms.layout.value());
        println!("    entity count:             {}", ms.entities.len());
    }
    if let Some(ps) = doc.block_records.get("*Paper_Space") {
        println!("  *Paper_Space BR handle:     {:#06X}  header: {:#06X}  {}",
            ps.handle.value(),
            h.paper_space_block_handle.value(),
            if ps.handle == h.paper_space_block_handle { "OK" } else { "MISMATCH!" });
        println!("    block_entity_handle:      {:#06X}", ps.block_entity_handle.value());
        println!("    block_end_handle:         {:#06X}", ps.block_end_handle.value());
        println!("    layout:                   {:#06X}", ps.layout.value());
    }

    // Check objects map
    println!("\n-- Objects map ({label}) --");
    println!("  Total objects: {}", doc.objects.len());
    
    // Check if all header-referenced dict handles exist in objects
    let dict_handles = [
        ("root_dict", h.named_objects_dict_handle),
        ("acad_group", h.acad_group_dict_handle),
        ("acad_mlinestyle", h.acad_mlinestyle_dict_handle),
        ("acad_layout", h.acad_layout_dict_handle),
        ("acad_plotsettings", h.acad_plotsettings_dict_handle),
        ("acad_plotstylename", h.acad_plotstylename_dict_handle),
        ("acad_material", h.acad_material_dict_handle),
        ("acad_color", h.acad_color_dict_handle),
        ("acad_visualstyle", h.acad_visualstyle_dict_handle),
    ];
    for (name, handle) in &dict_handles {
        let exists = doc.objects.contains_key(handle);
        println!("  {}: {:#06X} → {}", name, handle.value(),
            if exists { "EXISTS" } else { "MISSING!" });
    }

    // Check entity owner handles
    println!("\n-- Entity owner handles ({label}) --");
    for entity in doc.entities() {
        let c = entity.common();
        let owner_ok = if c.owner_handle.is_null() { "NULL!" } else { "OK" };
        let etype = format!("{:?}", entity).chars().take(30).collect::<String>();
        println!("  handle={:#06X} owner={:#06X} layer='{}' {} {}",
            c.handle.value(), c.owner_handle.value(), c.layer,
            etype, owner_ok);
    }

    // Check block record entities
    println!("\n-- Block record entity owner handles ({label}) --");
    for br in doc.block_records.iter() {
        if br.entities.is_empty() { continue; }
        println!("  Block '{}' (handle={:#06X}): {} entities",
            br.name, br.handle.value(), br.entities.len());
        for e in &br.entities {
            let c = e.common();
            let owner_ok = if c.owner_handle.is_null() {
                "NULL!"
            } else if c.owner_handle == br.handle {
                "OK"
            } else {
                "WRONG_OWNER!"
            };
            let etype = format!("{:?}", e).chars().take(30).collect::<String>();
            println!("    handle={:#06X} owner={:#06X} {} {}",
                c.handle.value(), c.owner_handle.value(),
                etype, owner_ok);
        }
    }

    // List all objects with their types
    println!("\n-- All objects ({label}) --");
    let mut obj_handles: Vec<_> = doc.objects.keys().collect();
    obj_handles.sort_by_key(|h| h.value());
    for handle in obj_handles {
        let obj = &doc.objects[handle];
        let oname = format!("{:?}", obj).chars().take(40).collect::<String>();
        println!("  {:#06X}: {}", handle.value(), oname);
    }
}
