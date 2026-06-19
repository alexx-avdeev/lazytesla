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

        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            audience,
            callback_port,
            domain,
        })
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