use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::vehicle_command::error::{Result, VehicleCommandError};

pub const USER_AGENT: &str = "lazytesla/0.1.0 tesla-sdk/rust";

pub struct FleetTransport {
    http: Client,
    base_url: String,
    server_host: String,
}

impl FleetTransport {
    pub fn new(base_url: String) -> Self {
        let trimmed = base_url.trim_end_matches('/').to_string();
        let server_host = trimmed
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string();
        Self {
            http: Client::builder()
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(30))
                .build()
                .expect("http client"),
            base_url: trimmed,
            server_host,
        }
    }

    pub fn server_host(&self) -> &str {
        &self.server_host
    }

    pub async fn wake_up(&mut self, vin: &str, access_token: &str) -> Result<()> {
        loop {
            let endpoint = format!("api/1/vehicles/{vin}/wake_up");
            match self.post_fleet(&endpoint, access_token, None).await {
                Ok(body) => {
                    if body.contains("\"state\":\"online\"") {
                        return Ok(());
                    }
                }
                Err(VehicleCommandError::FleetApi { status: 503, .. })
                | Err(VehicleCommandError::FleetApi { status: 408, .. }) => {}
                Err(err) => return Err(err),
            }

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    pub async fn signed_command(
        &mut self,
        vin: &str,
        access_token: &str,
        routable_message: &[u8],
    ) -> Result<Vec<u8>> {
        let endpoint = format!("api/1/vehicles/{vin}/signed_command");
        let request = SignedCommandRequest {
            routable_message: STANDARD.encode(routable_message),
        };
        let body = serde_json::to_string(&request)?;
        let response_body = self.post_fleet(&endpoint, access_token, Some(body)).await?;
        let parsed: SignedCommandResponse = serde_json::from_str(&response_body)?;
        let response = parsed
            .response
            .or(parsed.nested_response)
            .ok_or_else(|| VehicleCommandError::Protocol("missing signed_command response".into()))?;
        STANDARD
            .decode(response)
            .map_err(|err| VehicleCommandError::Protocol(format!("invalid base64 response: {err}")))
    }

    async fn post_fleet(
        &mut self,
        endpoint: &str,
        access_token: &str,
        body: Option<String>,
    ) -> Result<String> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let mut request = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {access_token}"))
            .header("Content-Type", "application/json")
            .header("Accept", "*/*");

        if let Some(ref payload) = body {
            request = request.body(payload.clone());
        }

        let response = request.send().await?;
        let status = response.status().as_u16();
        let text = response.text().await?;

        if status == 421 {
            if let Some(domain) = extract_redirect_domain(&text) {
                if domain.ends_with(".tesla.com")
                    || domain.ends_with(".tesla.cn")
                    || domain.ends_with(".teslamotors.com")
                {
                    self.base_url = format!("https://{domain}");
                    self.server_host = domain;
                    return Box::pin(self.post_fleet(endpoint, access_token, body)).await;
                }
            }
        }

        match status {
            200 => Ok(text),
            422 => Err(VehicleCommandError::Protocol(
                "vehicle command protocol not supported".into(),
            )),
            503 | 408 => Err(VehicleCommandError::VehicleUnavailable(text)),
            _ => Err(VehicleCommandError::FleetApi {
                status,
                body: text,
            }),
        }
    }
}

#[derive(Serialize)]
struct SignedCommandRequest {
    routable_message: String,
}

#[derive(Deserialize)]
struct SignedCommandResponse {
    response: Option<String>,
    #[serde(default)]
    nested_response: Option<String>,
}

fn extract_redirect_domain(body: &str) -> Option<String> {
    let marker = "use base URL: https://";
    let start = body.find(marker)? + marker.len();
    let rest = &body[start..];
    let end = rest.find(|c: char| c.is_whitespace() || c == ',')?;
    Some(rest[..end].trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_redirect_domain_from_421_body() {
        let body = r#"{"error":"user out of region, use base URL: https://fleet-api.prd.eu.vn.cloud.tesla.com, see docs"}"#;
        assert_eq!(
            extract_redirect_domain(body),
            Some("fleet-api.prd.eu.vn.cloud.tesla.com".into())
        );
    }
}