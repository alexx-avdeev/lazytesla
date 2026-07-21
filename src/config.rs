use std::path::Path;

use directories::BaseDirs;

use crate::error::{AppError, Result};

pub const DEFAULT_REDIRECT_URI: &str = "http://localhost:8484/callback";
pub const DEFAULT_AUDIENCE: &str = "https://fleet-api.prd.na.vn.cloud.tesla.com";
pub const DEFAULT_CALLBACK_PORT: u16 = 8484;

pub const SCOPES: &str =
    "openid offline_access user_data vehicle_device_data vehicle_cmds vehicle_charging_cmds";

#[derive(Debug, Clone)]
pub struct Config {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub audience: String,
    pub callback_port: u16,
    pub domain: Option<String>,
    pub command_proxy_url: Option<String>,
    pub command_proxy_ca_cert: Option<String>,
    pub fleet_key_path: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let client_id = std::env::var("TESLA_CLIENT_ID")
            .map_err(|_| AppError::Config("TESLA_CLIENT_ID is required".into()))?;
        let client_secret = std::env::var("TESLA_CLIENT_SECRET")
            .map_err(|_| AppError::Config("TESLA_CLIENT_SECRET is required".into()))?;

        let redirect_uri = std::env::var("TESLA_REDIRECT_URI")
            .unwrap_or_else(|_| DEFAULT_REDIRECT_URI.to_string());
        let audience =
            std::env::var("TESLA_AUDIENCE").unwrap_or_else(|_| DEFAULT_AUDIENCE.to_string());
        let callback_port = std::env::var("TESLA_CALLBACK_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_CALLBACK_PORT);
        let domain = std::env::var("TESLA_DOMAIN").ok().filter(|value| !value.is_empty());
        let command_proxy_url = std::env::var("TESLA_COMMAND_PROXY_URL")
            .ok()
            .filter(|value| !value.is_empty())
            .map(|url| Self::normalize_command_proxy_url(&url));
        let command_proxy_ca_cert = std::env::var("TESLA_COMMAND_PROXY_CA_CERT")
            .ok()
            .filter(|value| !value.is_empty())
            .map(|path| Self::expand_path(&path));
        let fleet_key_path = std::env::var("TESLA_FLEET_KEY")
            .or_else(|_| std::env::var("TESLA_KEY_FILE"))
            .ok()
            .filter(|value| !value.is_empty())
            .map(|path| Self::expand_path(&path));

        let config = Self {
            client_id,
            client_secret,
            redirect_uri,
            audience,
            callback_port,
            domain,
            command_proxy_url,
            command_proxy_ca_cert,
            fleet_key_path,
        };

        config.validate_command_proxy()?;
        config.validate_fleet_key()?;
        Ok(config)
    }

    fn validate_command_proxy(&self) -> Result<()> {
        let Some(proxy_url) = &self.command_proxy_url else {
            return Ok(());
        };

        let Some(cert_path) = &self.command_proxy_ca_cert else {
            return Err(AppError::Config(format!(
                "TESLA_COMMAND_PROXY_URL is set to {proxy_url} but TESLA_COMMAND_PROXY_CA_CERT is \
                 missing. Set it to the absolute path of your proxy tls-cert.pem, e.g. \
                 /Users/you/lazytesla/config/tls-cert.pem"
            )));
        };

        if !Path::new(cert_path).is_file() {
            return Err(AppError::Config(format!(
                "TESLA_COMMAND_PROXY_CA_CERT does not exist: {cert_path}"
            )));
        }

        Ok(())
    }

    fn normalize_command_proxy_url(url: &str) -> String {
        // tesla-http-proxy listens on IPv4 only; macOS resolves localhost to ::1 first.
        if let Ok(mut parsed) = url::Url::parse(url) {
            if parsed.host_str() == Some("localhost") {
                let _ = parsed.set_host(Some("127.0.0.1"));
                let mut normalized = parsed.to_string();
                if normalized.ends_with('/') {
                    normalized.pop();
                }
                return normalized;
            }
        }

        url.replace("://localhost:", "://127.0.0.1:")
    }

    fn expand_path(path: &str) -> String {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
                return home.join(rest).to_string_lossy().into_owned();
            }
        }

        if path == "~" {
            if let Some(home) = BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
                return home.to_string_lossy().into_owned();
            }
        }

        path.to_string()
    }

    fn validate_fleet_key(&self) -> Result<()> {
        let Some(path) = &self.fleet_key_path else {
            return Ok(());
        };

        if !Path::new(path).is_file() {
            return Err(AppError::Config(format!(
                "TESLA_FLEET_KEY does not exist: {path}"
            )));
        }

        Ok(())
    }

    pub fn uses_native_commands(&self) -> bool {
        self.fleet_key_path.is_some()
    }

    pub fn command_transport_label(&self) -> &'static str {
        if self.fleet_key_path.is_some() {
            "native signing (TESLA_FLEET_KEY)"
        } else if self.command_proxy_url.is_some() {
            "command proxy"
        } else {
            "unsigned Fleet API"
        }
    }

    pub fn command_proxy_help() -> String {
        "Vehicle commands require Tesla's Vehicle Command Protocol. \
         Set TESLA_FLEET_KEY to your fleet private key PEM (from tesla-keygen create), \
         or run tesla-http-proxy and set TESLA_COMMAND_PROXY_URL (e.g. https://127.0.0.1:4443) \
         plus TESLA_COMMAND_PROXY_CA_CERT. \
         Pair your app key on the vehicle via https://tesla.com/_ak/<your_domain>."
            .into()
    }

    pub fn registration_help(&self) -> String {
        format!(
            "Set TESLA_DOMAIN to your app's registered domain (from developer.tesla.com), \
             host your public key at \
             https://<domain>/.well-known/appspecific/com.tesla.3p.public-key.pem, \
             then press r to refresh. Current region: {}",
            self.audience
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, DEFAULT_AUDIENCE, DEFAULT_CALLBACK_PORT, DEFAULT_REDIRECT_URI};

    #[test]
    fn normalizes_localhost_proxy_url_to_ipv4() {
        let url = Config::normalize_command_proxy_url("https://localhost:4443");
        assert_eq!(url, "https://127.0.0.1:4443");
    }

    #[test]
    fn native_commands_take_priority_over_proxy_settings() {
        let config = Config {
            client_id: "id".into(),
            client_secret: "secret".into(),
            redirect_uri: DEFAULT_REDIRECT_URI.to_string(),
            audience: DEFAULT_AUDIENCE.to_string(),
            callback_port: DEFAULT_CALLBACK_PORT,
            domain: None,
            command_proxy_url: Some("https://127.0.0.1:4443".into()),
            command_proxy_ca_cert: Some("/tmp/cert.pem".into()),
            fleet_key_path: Some("/tmp/key.pem".into()),
        };

        assert!(config.uses_native_commands());
        assert_eq!(
            config.command_transport_label(),
            "native signing (TESLA_FLEET_KEY)"
        );
    }
}