use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use reqwest::Client;
use serde::Deserialize;

use crate::config::{Config, SCOPES};
use crate::error::{AppError, Result};

const AUTHORIZE_URL: &str = "https://auth.tesla.com/oauth2/v3/authorize";
const TOKEN_URL: &str = "https://fleet-auth.prd.vn.cloud.tesla.com/oauth2/v3/token";

#[derive(Debug, Clone)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

pub struct OAuthClient {
    config: Config,
    http: Client,
}

impl OAuthClient {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            http: Client::new(),
        }
    }

    pub fn generate_state() -> String {
        let mut rng = rand::thread_rng();
        (0..32)
            .map(|_| {
                let idx = rng.gen_range(0..62);
                b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"[idx] as char
            })
            .collect()
    }

    pub fn authorize_url(&self, state: &str) -> String {
        let mut url = url::Url::parse(AUTHORIZE_URL).expect("valid authorize URL");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("client_id", &self.config.client_id);
            query.append_pair("redirect_uri", &self.config.redirect_uri);
            query.append_pair("scope", SCOPES);
            query.append_pair("state", state);
            query.append_pair("prompt_missing_scopes", "true");
        }
        url.to_string()
    }

    pub async fn exchange_code(&self, code: &str) -> Result<TokenSet> {
        let response = self
            .http
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("code", code),
                ("audience", self.config.audience.as_str()),
                ("redirect_uri", self.config.redirect_uri.as_str()),
            ])
            .send()
            .await?;

        self.parse_token_response(response).await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenSet> {
        let response = self
            .http
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", self.config.client_id.as_str()),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await?;

        self.parse_token_response(response).await
    }

    async fn parse_token_response(&self, response: reqwest::Response) -> Result<TokenSet> {
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(AppError::Auth(format!(
                "token request failed ({status}): {body}"
            )));
        }

        let token: TokenResponse = serde_json::from_str(&body).map_err(|err| {
            AppError::Auth(format!("failed to parse token response: {err}: {body}"))
        })?;

        Ok(TokenSet {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at: Utc::now() + Duration::seconds(token.expires_in as i64),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::OAuthClient;
    use crate::config::Config;

    fn test_config() -> Config {
        Config {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            redirect_uri: "http://localhost:8484/callback".into(),
            audience: "https://fleet-api.prd.na.vn.cloud.tesla.com".into(),
            callback_port: 8484,
            domain: Some("example.com".into()),
            command_proxy_url: None,
            command_proxy_ca_cert: None,
            fleet_key_path: None,
        }
    }

    #[test]
    fn authorize_url_includes_required_oauth_params() {
        let client = OAuthClient::new(test_config());
        let url = client.authorize_url("test-state");

        assert!(url.starts_with("https://auth.tesla.com/oauth2/v3/authorize"));
        assert!(url.contains("client_id=test-client"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=test-state"));
        assert!(url.contains("prompt_missing_scopes=true"));
    }

    #[test]
    fn generate_state_is_32_characters() {
        assert_eq!(OAuthClient::generate_state().len(), 32);
    }
}