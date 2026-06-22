use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use crate::api::debug_curl;
use crate::api::details::{VehicleDataResponse, VehicleDetails};
use crate::api::vehicles::{Vehicle, VehiclesResponse};
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

#[cfg(test)]
mod tests {
    use super::{needs_partner_registration, parse_error_message};

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
}