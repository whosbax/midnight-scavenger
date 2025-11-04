// src/config.rs
// Module to load application configuration (API base URL, wallet info, address, etc).
// Uses the `config` crate to merge file + environment variables.

use serde::Deserialize;
use std::error::Error;

/// Top‑level configuration struct for the application.
///
/// All fields are loaded from configuration sources.
/// Make sure to set defaults or provide environment / file values.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Base URL of the Scavenger Mine API (e.g. https://scavenger.prod.gd.midnighttge.io)
    pub base_url: String,

    /// Wallet address (Cardano payment address) to be used for this miner
    pub address: String,

    /// Path to the wallet private key (or key file) for signing
    pub wallet_key_path: String,

    /// Logging level (e.g. "info", "debug")
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    /// Load configuration from file `config.toml` (optional) and ENV variables.
    /// Environment variables take precedence and must use prefix `APP_`.
    ///
    /// Example environment variables:
    ///   APP_BASE_URL=https://...
    ///   APP_ADDRESS=addr1q...
    ///   APP_WALLET_KEY_PATH=/path/to/key
    ///   APP_LOG_LEVEL=debug
    pub fn load() -> Result<Self, Box<dyn Error>> {
        // Create builder
        let builder = config::Config::builder()
            // Optionally load config file. Use "config.toml" at working directory.
            .add_source(config::File::with_name("config").required(false))
            // Merge in environment variables with prefix "APP_"
            .add_source(config::Environment::with_prefix("APP").separator("_"));

        // Build the configuration
        let cfg = builder.build()?;

        // Deserialize into our struct
        let settings: Config = cfg.try_deserialize()?;

        Ok(settings)
    }
}
