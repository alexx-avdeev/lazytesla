use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use crate::api::debug_curl;
use crate::api::details::{VehicleDataResponse, VehicleDetails};
use crate::api::vehicles::{Vehicle, VehiclesResponse};
use crate::config::Config;
use crate::error::{AppError, Result};

pub struct FleetClient {
    base_url: String,
    http: Client,
}

impl FleetClient {
    pub fn new(base_url: String) -> Self {
        Self::with_http(base_url, Client::new())
    }

    pub fn with_http(base_url: String, http: Client) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        }
    }

    pub fn for_config(config: &Config) -> Result<Self> {
        if let Some(url) = &config.command_proxy_url {
            Self::with_tls(url.clone(), config.command_proxy_ca_cert.as_deref())
        } else {
            Ok(Self::new(config.audience.clone()))
        }
    }

    pub fn with_tls(base_url: String, ca_cert_path: Option<&str>) -> Result<Self> {
        let path = ca_cert_path.ok_or_else(|| {
            AppError::Config(
                "TESLA_COMMAND_PROXY_CA_CERT is required when TESLA_COMMAND_PROXY_URL is set"
                    .into(),
            )
        })?;

        let pem = std::fs::read(path).map_err(|err| {
            AppError::Config(format!(
                "failed to read TESLA_COMMAND_PROXY_CA_CERT at {path}: {err}"
            ))
        })?;

        let cert = reqwest::Certificate::from_pem(&pem).map_err(|err| {
            AppError::Config(format!(
                "invalid PEM in TESLA_COMMAND_PROXY_CA_CERT ({path}): {err}"
            ))
        })?;

        let http = Client::builder()
            .add_root_certificate(cert)
            .build()?;
        Ok(Self::with_http(base_url, http))
    }

    pub async fn register_partner(&self, partner_token: &str, domain: &str) -> Result<()> {
        let url = format!("{}/api/1/partner_accounts", self.base_url);
        let body = json!({ "domain": domain }).to_string();
        let auth = format!("Bearer {partner_token}");
        debug_curl::log_post_json(
            &url,
            &[
                ("Authorization", auth.as_str()),
                ("Content-Type", "application/json"),
            ],
            &body,
        );

        let response = self
            .http
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            return Ok(());
        }

        let message = parse_error_message(&body)
            .unwrap_or_else(|| format!("partner registration failed ({status})"));
        Err(AppError::Api(format!("{message}: {body}")))
    }

    pub async fn get_vehicle_data(&self, vin: &str, access_token: &str) -> Result<VehicleDetails> {
        let url = format!("{}/api/1/vehicles/{vin}/vehicle_data", self.base_url);
        let auth = format!("Bearer {access_token}");
        debug_curl::log_get(
            &url,
            &[
                ("Authorization", auth.as_str()),
                ("Content-Type", "application/json"),
            ],
        );

        let response = self
            .http
            .get(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let message = parse_error_message(&body)
                .unwrap_or_else(|| format!("request failed ({status})"));
            return Err(AppError::Api(format!("{message}: {body}")));
        }

        let parsed: VehicleDataResponse = serde_json::from_str(&body).map_err(|err| {
            AppError::Api(format!("failed to parse vehicle data response: {err}: {body}"))
        })?;

        Ok(VehicleDetails::from_raw(parsed.response))
    }

    pub async fn send_command(
        &self,
        vin: &str,
        command: &str,
        access_token: &str,
        proxy_configured: bool,
    ) -> Result<()> {
        let url = format!("{}/api/1/vehicles/{vin}/command/{command}", self.base_url);
        let body = "{}";
        let auth = format!("Bearer {access_token}");
        debug_curl::log_post_json(
            &url,
            &[
                ("Authorization", auth.as_str()),
                ("Content-Type", "application/json"),
            ],
            body,
        );

        let response = self
            .http
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(format_proxy_http_error)?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let message = parse_error_message(&body)
                .unwrap_or_else(|| format!("request failed ({status})"));
            return Err(map_command_error(format!("{message}: {body}"), proxy_configured));
        }

        let parsed: CommandResponse = serde_json::from_str(&body).map_err(|err| {
            AppError::Api(format!("failed to parse command response: {err}: {body}"))
        })?;

        if !parsed.response.result {
            let reason = if parsed.response.reason.is_empty() {
                "command rejected by vehicle".into()
            } else {
                parsed.response.reason
            };
            return Err(map_command_error(reason, proxy_configured));
        }

        Ok(())
    }

    pub async fn list_vehicles(&self, access_token: &str) -> Result<Vec<Vehicle>> {
        let url = format!("{}/api/1/vehicles", self.base_url);
        let auth = format!("Bearer {access_token}");
        debug_curl::log_get(
            &url,
            &[
                ("Authorization", auth.as_str()),
                ("Content-Type", "application/json"),
            ],
        );

        let response = self
            .http
            .get(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .send()
            .await?;

        self.parse_vehicles_response(response).await
    }

    async fn parse_vehicles_response(&self, response: reqwest::Response) -> Result<Vec<Vehicle>> {
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let message = parse_error_message(&body)
                .unwrap_or_else(|| format!("request failed ({status})"));
            return Err(AppError::Api(format!("{message}: {body}")));
        }

        let parsed: VehiclesResponse = serde_json::from_str(&body).map_err(|err| {
            AppError::Api(format!("failed to parse vehicles response: {err}: {body}"))
        })?;

        Ok(parsed.response.into_iter().map(Vehicle::from).collect())
    }
}

#[derive(Debug, Deserialize)]
struct CommandResponse {
    response: CommandResult,
}

#[derive(Debug, Deserialize)]
struct CommandResult {
    result: bool,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    error: Option<String>,
    error_description: Option<String>,
}

pub fn parse_error_message(body: &str) -> Option<String> {
    let parsed: ApiErrorBody = serde_json::from_str(body).ok()?;
    match (parsed.error, parsed.error_description) {
        (Some(error), Some(description)) => Some(format!("{error}: {description}")),
        (Some(error), None) => Some(error),
        (None, Some(description)) => Some(description),
        (None, None) => None,
    }
}

pub fn needs_partner_registration(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("must be registered")
        || lower.contains("unregistered account")
        || lower.contains("partner_accounts")
}

fn format_proxy_http_error(err: reqwest::Error) -> AppError {
    let details = format!("{err:?}");
    let lower = details.to_ascii_lowercase();

    // rustls reports handshake/cert failures as Connect errors; check TLS first.
    if lower.contains("causedasendentity") {
        return AppError::Config(
            "TLS certificate rejected: tls-cert.pem was generated as a CA certificate \
             (CA:TRUE). Regenerate it as a server certificate with SAN entries for \
             localhost and 127.0.0.1 — see README step 2 — then restart tesla-http-proxy."
                .into(),
        );
    }

    if lower.contains("certificate") || lower.contains("invalidcertificate") {
        return AppError::Config(format!(
            "TLS error connecting to command proxy: {details}. \
             Ensure TESLA_COMMAND_PROXY_CA_CERT points to your proxy tls-cert.pem and \
             TESLA_COMMAND_PROXY_URL=https://127.0.0.1:4443."
        ));
    }

    if err.is_connect() {
        return AppError::Api(format!(
            "could not connect to command proxy ({err}). \
             Is tesla-http-proxy running? Try TESLA_COMMAND_PROXY_URL=https://127.0.0.1:4443 \
             instead of localhost."
        ));
    }

    AppError::Http(err)
}

fn map_command_error(message: String, proxy_configured: bool) -> AppError {
    if !proxy_configured && needs_vehicle_command_protocol(&message) {
        AppError::Config(Config::command_proxy_help())
    } else {
        AppError::Api(message)
    }
}

pub fn needs_vehicle_command_protocol(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("vehicle command protocol")
}

#[cfg(test)]
mod tests {
    use crate::error::AppError;

    use super::{needs_partner_registration, needs_vehicle_command_protocol, parse_error_message};

    #[test]
    fn parses_api_error_with_description() {
        let body = r#"{"error":"invalid_request","error_description":"Account must be registered"}"#;
        assert_eq!(
            parse_error_message(body),
            Some("invalid_request: Account must be registered".into())
        );
    }

    #[test]
    fn detects_registration_required_errors() {
        assert!(needs_partner_registration(
            "Account must be registered in the current region"
        ));
        assert!(needs_partner_registration("Unregistered account"));
        assert!(!needs_partner_registration("vehicle is asleep"));
    }

    #[test]
    fn detects_vehicle_command_protocol_errors() {
        assert!(needs_vehicle_command_protocol(
            "Tesla Vehicle Command Protocol required, please refer to the documentation"
        ));
    }

    #[test]
    fn maps_vehicle_command_protocol_error_to_config_help() {
        let err = super::map_command_error(
            "Tesla Vehicle Command Protocol required".into(),
            false,
        );
        assert!(matches!(err, AppError::Config(_)));
        assert!(err.to_string().contains("TESLA_COMMAND_PROXY_URL"));
    }

    #[test]
    fn with_tls_builds_from_server_certificate_pem() {
        let cert_path = format!("{}/config/tls-cert.pem", env!("CARGO_MANIFEST_DIR"));
        if !std::path::Path::new(&cert_path).exists() {
            return;
        }

        let client = super::FleetClient::with_tls("https://127.0.0.1:4443".into(), Some(&cert_path));
        assert!(client.is_ok(), "{}", client.err().unwrap());
    }
}