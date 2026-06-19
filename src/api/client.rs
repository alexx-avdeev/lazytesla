use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

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
        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {partner_token}"))
            .header("Content-Type", "application/json")
            .json(&json!({ "domain": domain }))
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

    pub async fn list_vehicles(&self, access_token: &str) -> Result<Vec<Vehicle>> {
        let url = format!("{}/api/1/vehicles", self.base_url);
        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {access_token}"))
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