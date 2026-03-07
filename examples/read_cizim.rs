use acadrust::io::dwg::DwgReader;
use acadrust::entities::EntityType;

fn main() -> acadrust::Result<()> {
    let path = "Çizim1.dwg";
    let mut reader = DwgReader::from_file(path)?;
    let doc = reader.read()?;
    
    println!("Version: {:?}", doc.version);
    println!("Entities: {}", doc.entities().count());
    println!("Layers: {}", doc.layers.len());
    println!();
    
    // Count by type
    let mut counts = std::collections::HashMap::new();
    for e in doc.entities() {
        let name = match e {
            EntityType::Line(_) => "LINE",
            EntityType::Circle(_) => "CIRCLE",
            EntityType::Arc(_) => "ARC",
            EntityType::LwPolyline(_) => "LWPOLYLINE",
            EntityType::Polyline2D(_) => "POLYLINE2D",
            EntityType::Polyline3D(_) => "POLYLINE3D",
            EntityType::Text(_) => "TEXT",
            EntityType::MText(_) => "MTEXT",
            EntityType::Spline(_) => "SPLINE",
            EntityType::Ellipse(_) => "ELLIPSE",
            EntityType::Hatch(_) => "HATCH",
            EntityType::Insert(_) => "INSERT",
            EntityType::Dimension(_) => "DIMENSION",
            EntityType::Solid(_) => "SOLID",
            EntityType::Point(_) => "POINT",
            EntityType::Solid3D(_) => "SOLID3D",
            EntityType::Mesh(_) => "MESH",
            EntityType::Face3D(_) => "FACE3D",
            EntityType::Leader(_) => "LEADER",
            EntityType::MultiLeader(_) => "MULTILEADER",
            EntityType::Viewport(_) => "VIEWPORT",
            EntityType::Table(_) => "TABLE",
            _ => "OTHER",
        };
        *counts.entry(name).or_insert(0u32) += 1;
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    println!("Entity breakdown:");
    for (name, count) in &sorted {
        println!("  {:<16} {}", name, count);
    }
    
    println!("\nLayers:");
    for layer in doc.layers.iter() {
        println!("  {:<20} color={:?}  frozen={}", layer.name, layer.color, layer.is_frozen());
    }
    
    // Notifications
    let notes: Vec<_> = doc.notifications.iter().collect();
    if !notes.is_empty() {
        println!("\nNotifications: {}", notes.len());
        for n in notes.iter().take(10) {
            println!("  [{:?}] {}", n.notification_type, n.message);
        }
        if notes.len() > 10 {
            println!("  ... and {} more", notes.len() - 10);
        }
    }
    
    Ok(())
}
