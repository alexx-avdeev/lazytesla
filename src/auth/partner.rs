use reqwest::Client;
use serde::Deserialize;

use crate::config::Config;
use crate::error::{AppError, Result};

pub const TOKEN_URL: &str = "https://fleet-auth.prd.vn.cloud.tesla.com/oauth2/v3/token";
const PARTNER_SCOPES: &str = "openid";

#[derive(Debug, Deserialize)]
struct PartnerTokenResponse {
    access_token: String,
}

pub struct PartnerAuth {
    config: Config,
    http: Client,
    token_url: String,
}

impl PartnerAuth {
    pub fn new(config: Config) -> Self {
        Self::with_options(config, Client::new(), TOKEN_URL.to_string())
    }

    pub fn with_options(config: Config, http: Client, token_url: String) -> Self {
        Self {
            config,
            http,
            token_url,
        }
    }

    pub async fn partner_token(&self) -> Result<String> {
        let response = self
            .http
            .post(&self.token_url)
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