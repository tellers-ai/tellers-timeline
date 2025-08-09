use schemars::schema_for;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schema_for!(tellers_timeline_core::types::Timeline);
    let schema_json = serde_json::to_string_pretty(&schema)?;
    let out_path = "spec/otio.schema.json";
    if let Some(parent) = std::path::Path::new(out_path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out_path, schema_json)?;
    println!("wrote {}", out_path);
    Ok(())
}
