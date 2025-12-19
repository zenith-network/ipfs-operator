use std::{fs::File, io::BufReader, path::Path};

use serde_json::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(Path::new("data/config"))?;
    let reader = BufReader::new(file);

    let mut config: serde_json::Value = serde_json::from_reader(reader)?;
    config["Identity"]["PeerID"] = Value::String("PeerIDTest".to_string());

    println!("{config:?}");

    Ok(())
}
