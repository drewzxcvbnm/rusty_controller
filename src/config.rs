use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub application_port_path: String,
    pub pump_port_path: String,
    pub router_port_path: String,
    pub constant_cleaning: bool,
}

lazy_static! {
    pub static ref CONFIG: Config = std::fs::read_to_string("./config.toml")
        .map_err(|e| e.to_string())
        .and_then(|s| toml::from_str(s.as_str()).map_err(|e| e.to_string()))
        .expect("Unable to load configuration file");
}
