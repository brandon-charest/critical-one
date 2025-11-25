use ::config::{ConfigError, Environment, File};
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
    pub fn load() -> Result<Self, ConfigError> {
        let env = env::var("RUN_ENV").unwrap_or_else(|_| "default".into());
        let mut builder = ::config::Config::builder().add_source(File::with_name("config/default.toml"));

        if env == "production" {
            builder = builder.add_source(File::with_name("config/production.toml").required(true));
        } else if env == "local" {
            builder = builder.add_source(File::with_name("config/local.toml").required(false));
        }

        builder = builder.add_source(Environment::with_prefix("APP").separator("__"));
        builder.build()?.try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    #[test]
    #[serial]
    fn test_load_defaults() {
        env::remove_var("RUN_ENV");
        env::remove_var("APP__SERVER__ADDR");

        let config = Config::load().expect("Failed to load config.");
        assert_eq!(config.database.redis_url, "redis://127.0.0.1:6379/");
        assert_eq!(config.logging.level, "info,critical_one=debug,tower_http=debug");
        assert_eq!(config.server.addr, "127.0.0.1:3000");
    }

    #[test]
    #[serial]
    fn test_env_variable_override() {
        env::set_var("APP__SERVER__ADDR", "1.2.3.4:9999");
        env::set_var("RUN_ENV", "production");

        let config = Config::load().expect("Failed to load config");

        // Assert that the environment variable won
        assert_eq!(config.server.addr, "1.2.3.4:9999");
    }

    #[test]
    #[serial]
    fn test_env_load_production() {
        env::set_var("RUN_ENV", "production");

        let config = Config::load().expect("Failed to load config");

        // Assert that the environment variable won
        assert_eq!(config.server.addr, "127.0.0.1:8080");
    }
}
