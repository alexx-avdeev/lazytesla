use reqwest::Client;
use serde::Deserialize;

use crate::config::Config;
use crate::error::{AppError, Result};

const TOKEN_URL: &str = "https://fleet-auth.prd.vn.cloud.tesla.com/oauth2/v3/token";
const PARTNER_SCOPES: &str = "openid";

#[derive(Debug, Deserialize)]
struct PartnerTokenResponse {
    access_token: String,
}

pub struct PartnerAuth {
    config: Config,
    http: Client,
}

impl PartnerAuth {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            http: Client::new(),
        }
    }

    pub async fn partner_token(&self) -> Result<String> {
        let response = self
            .http
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("audience", self.config.audience.as_str()),
                ("scope", PARTNER_SCOPES),
            ])
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(AppError::Auth(format!(
                "partner token request failed ({status}): {body}"
            )));
        }

        let token: PartnerTokenResponse = serde_json::from_str(&body).map_err(|err| {
            AppError::Auth(format!("failed to parse partner token response: {err}: {body}"))
        })?;

        Ok(token.access_token)
    }
}