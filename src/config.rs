use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct TubeHolderCoordinates {
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub application_port_path: String,
    pub pump_port_path: String,
    pub router_port_path: String,
    pub constant_cleaning: bool,
    #[serde(rename(deserialize = "tube-holder-coordinates"))]
    pub tube_holder_coordinates: HashMap<String, String>,
}

static DEFAULT_CONFIG: &str = include_str!("../config.toml");

fn load_config() -> Config {
    if !Path::new("./config.toml").exists() {
        File::create(Path::new("./config.toml"))
            .and_then(|mut f| f.write(DEFAULT_CONFIG.as_bytes()))
            .expect("Failed to create config file");
        log::error!("config.toml file not found. Creating new one and using default configs");
    }
    std::fs::read_to_string("./config.toml")
        .map_err(|e| e.to_string())
        .and_then(|s| toml::from_str(s.as_str()).map_err(|e| e.to_string()))
        .expect("Unable to load configuration file")
}

lazy_static! {
    pub static ref CONFIG: Config = load_config();
}
