//! Writes `docs/otio-reference.md`; run from the repository root
//! (`just regen-docs`).

use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = tellers_timeline_schema::REFERENCE_PATH;
    if let Some(parent) = std::path::Path::new(out_path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out_path, tellers_timeline_schema::reference_markdown())?;
    println!("wrote {}", out_path);
    Ok(())
}
