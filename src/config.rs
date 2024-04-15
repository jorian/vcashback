use anyhow::{anyhow, Context, Result};
use secrecy::{ExposeSecret, Secret};
use serde::Deserialize;
use serde_aux::field_attributes::deserialize_number_from_string;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub discord: DiscordConfig,
    pub database: DbConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DiscordConfig {
    pub token: String,
    // pub channels: Map<String, u64>, // TODO
}

#[derive(Debug, Deserialize, Clone)]
pub struct DbConfig {
    #[serde(rename = "name")]
    pub db_name: String,
    #[serde(rename = "password")]
    pub db_pass: Secret<String>,
    #[serde(rename = "user")]
    pub db_user: String,
    #[serde(rename = "host")]
    pub db_host: String,
    #[serde(rename = "port")]
    pub db_port: u16,
}

impl DbConfig {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.db_user,
            self.db_pass.expose_secret(),
            self.db_host,
            self.db_port,
            self.db_name
        )
    }
}

pub fn get_configuration() -> Result<Config> {
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");
    let configuration_directory = base_path.join("config");

    // Detect the running environment.
    // Default to `local` if unspecified.
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT.");
    let environment_filename = format!("{}.toml", environment.as_str());
    let settings = config::Config::builder()
        .add_source(config::File::from(
            configuration_directory.join("base.toml"),
        ))
        .add_source(config::File::from(
            configuration_directory.join(&environment_filename),
        ))
        // Add in settings from environment variables (with a prefix of APP and '__' as separator)
        // E.g. `APP_APPLICATION__PORT=5001 would set `Settings.application.port`
        .add_source(
            config::Environment::with_prefix("APP")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()?;

    settings
        .try_deserialize::<Config>()
        .context("failed to serialize into config")
}

pub enum Environment {
    Local,
    Development,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Development => "dev",
            Environment::Production => "prod",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = anyhow::Error;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "dev" => Ok(Self::Development),
            "prod" => Ok(Self::Production),
            other => Err(anyhow!("{} is not a valid environment", other)),
        }
    }
}

pub mod pbaas {
    use std::path::PathBuf;

    use tracing::warn;
    use vrsc_rpc::json::vrsc::Address;

    use super::*;

    #[derive(Debug, Deserialize, Clone)]
    pub struct Config {
        pub rpc_user: String,
        pub rpc_password: String,
        #[serde(deserialize_with = "deserialize_number_from_string")]
        pub rpc_port: u16,
        pub zmq_block_hash_url: String,
        pub currency_id: Address,
        pub referral_currency_id: Address,
    }

    pub fn pbaas_chain_configs() -> Result<Vec<self::Config>> {
        let base_path = std::env::current_dir().expect("Failed to determine the current directory");
        let config_dir = base_path.join("pbaas");

        let mut pbaas_configs = vec![];

        if let Ok(dir) = config_dir.read_dir() {
            for entry in dir {
                let entry = entry?;
                let path = PathBuf::from(entry.file_name());

                if let Some(extension) = path.extension() {
                    if extension.eq_ignore_ascii_case("toml") {
                        let settings = config::Config::builder()
                            .add_source(config::File::from(config_dir.join(&path)))
                            .build()?
                            .try_deserialize::<self::Config>()?;

                        pbaas_configs.push(settings);
                    }
                }
            }
        } else {
            warn!("no `pbaas` directory set in root directory");
        }

        Ok(pbaas_configs)
    }
}
