use std::{error::Error, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let destination = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/api/generated");
    lizzieyzy_desktop::bindings::export(&destination)?;
    println!("bindings: exported to {}", destination.display());
    Ok(())
}
