use serde::Deserialize;
use std::error::Error;
use std::fs;

#[derive(Deserialize)]
pub struct Config {
    pub url: String,
    pub session_id: String,
}

impl Config {
    pub fn read() -> Result<Config, Box<dyn Error>> {
        let config_str = fs::read_to_string("config.json")?;
        let config: Config = serde_json::from_str(&config_str)?;

        Ok(config)
    }
}
