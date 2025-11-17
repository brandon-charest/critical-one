use serde::Deserialize;
use std::env;



#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub addr: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub redis_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}


#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
}

impl Config {

    pub fn load() -> Result<Self, config::ConfigError> {
        let env = env::var("RUN_ENV").unwrap_or_else(|_| "local".into());

        let builder = ::config::Config::builder()
            .add_source(config::File::with_name("config/default.toml"))
            .add_source(
                config::File::with_name(&format!("config/{}", env))
                    .required(false),
            )
            .add_source(config::File::with_name("config/local.toml").required(false))
            .add_source(config::Environment::with_prefix("APP").separator("__"));

        builder.build()?.try_deserialize()
    }
}