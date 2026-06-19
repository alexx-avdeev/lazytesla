use chrono::{DateTime, Utc};

use crate::api::{FleetApi, Vehicle};
use crate::auth::oauth::{OAuthClient, TokenSet};
use crate::auth::store::{StoredTokens, TokenStore};
use crate::config::Config;
use crate::error::{AppError, Result};

const LOGIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Auth,
    Home,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    NotAuthenticated,
    WaitingForBrowser,
    ExchangingToken,
    Authenticated,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VehiclesStatus {
    Idle,
    Loading,
    Loaded,
    Error(String),
}

pub struct App {
    pub screen: Screen,
    pub auth_status: AuthStatus,
    pub tokens: Option<StoredTokens>,
    pub status_message: String,
    pub vehicles: Vec<Vehicle>,
    pub vehicles_status: VehiclesStatus,
    pub selected_vehicle: usize,
    config: Config,
    oauth: OAuthClient,
    token_store: TokenStore,
    pending_state: Option<String>,
}

impl App {
    pub async fn new(config: Config) -> Result<Self> {
        let oauth = OAuthClient::new(config.clone());
        let token_store = TokenStore::new()?;

        let mut app = Self {
            screen: Screen::Auth,
            auth_status: AuthStatus::NotAuthenticated,
            tokens: None,
            status_message: "Press Enter to sign in with Tesla".into(),
            vehicles: Vec::new(),
            vehicles_status: VehiclesStatus::Idle,
            selected_vehicle: 0,
            config,
            oauth,
            token_store,
            pending_state: None,
        };

        app.try_restore_session().await?;
        Ok(app)
    }

    pub fn is_authenticated(&self) -> bool {
        self.screen == Screen::Home && self.tokens.is_some()
    }

    async fn try_restore_session(&mut self) -> Result<()> {
        let Some(stored) = self.token_store.load()? else {
            return Ok(());
        };

        self.status_message = "Restoring session...".into();
        self.auth_status = AuthStatus::ExchangingToken;

        let tokens = if TokenStore::is_expired(&stored) {
            self.oauth.refresh(&stored.refresh_token).await?
        } else {
            TokenSet {
                access_token: stored.access_token.clone(),
                refresh_token: stored.refresh_token.clone(),
                expires_at: stored.expires_at,
            }
        };

        self.set_authenticated(tokens).await?;
        Ok(())
    }

    pub fn start_login(&mut self) -> Result<(String, u16, String)> {
        if self.auth_status == AuthStatus::WaitingForBrowser {
            return Err(AppError::Auth("login already in progress".into()));
        }

        let state = OAuthClient::generate_state();
        let url = self.oauth.authorize_url(&state);
        self.pending_state = Some(state.clone());
        self.auth_status = AuthStatus::WaitingForBrowser;
        self.status_message =
            "Browser opened. Complete login in your browser, then return here.".into();

        Ok((url, self.config.callback_port, state))
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub async fn set_authenticated(&mut self, tokens: TokenSet) -> Result<()> {
        let stored = StoredTokens::from(tokens);
        self.token_store.save(&stored)?;
        self.tokens = Some(stored);
        self.screen = Screen::Home;
        self.auth_status = AuthStatus::Authenticated;
        self.status_message = "Signed in successfully".into();
        self.vehicles.clear();
        self.selected_vehicle = 0;
        self.vehicles_status = VehiclesStatus::Idle;
        Ok(())
    }

    pub fn begin_vehicle_load(&mut self) {
        self.vehicles_status = VehiclesStatus::Loading;
        self.status_message = "Loading vehicles...".into();
    }

    pub fn apply_vehicles(&mut self, result: Result<Vec<Vehicle>>) {
        match result {
            Ok(vehicles) => {
                self.selected_vehicle = 0;
                self.vehicles = vehicles;
                self.vehicles_status = VehiclesStatus::Loaded;
                self.status_message = if self.vehicles.is_empty() {
                    "No vehicles found on this account".into()
                } else {
                    format!("Loaded {} vehicle(s)", self.vehicles.len())
                };
            }
            Err(err) => {
                self.vehicles.clear();
                self.vehicles_status = VehiclesStatus::Error(err.to_string());
                self.status_message = err.to_string();
            }
        }
    }

    pub fn vehicle_load_request(&self) -> Option<VehicleLoadRequest> {
        let access_token = self.tokens.as_ref()?.access_token.clone();
        Some(VehicleLoadRequest {
            config: self.config.clone(),
            access_token,
        })
    }

    pub fn select_previous_vehicle(&mut self) {
        if self.vehicles.is_empty() {
            return;
        }
        self.selected_vehicle = self.selected_vehicle.saturating_sub(1);
    }

    pub fn select_next_vehicle(&mut self) {
        if self.vehicles.is_empty() {
            return;
        }
        let last = self.vehicles.len().saturating_sub(1);
        self.selected_vehicle = (self.selected_vehicle + 1).min(last);
    }

    pub fn logout(&mut self) -> Result<()> {
        self.token_store.clear()?;
        self.tokens = None;
        self.pending_state = None;
        self.vehicles.clear();
        self.selected_vehicle = 0;
        self.vehicles_status = VehiclesStatus::Idle;
        self.screen = Screen::Auth;
        self.auth_status = AuthStatus::NotAuthenticated;
        self.status_message = "Signed out. Press Enter to sign in again.".into();
        Ok(())
    }

    pub fn set_error(&mut self, error: AppError) {
        self.pending_state = None;
        self.auth_status = AuthStatus::Error(error.to_string());
        self.status_message = error.to_string();
        self.screen = Screen::Auth;
    }

    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.tokens.as_ref().map(|tokens| tokens.expires_at)
    }

    pub fn login_timeout(&self) -> std::time::Duration {
        LOGIN_TIMEOUT
    }
}

#[derive(Debug, Clone)]
pub struct VehicleLoadRequest {
    pub config: Config,
    pub access_token: String,
}

pub async fn fetch_vehicles(request: VehicleLoadRequest) -> Result<Vec<Vehicle>> {
    FleetApi::from_config(&request.config)
        .fetch_vehicles(&request.config, &request.access_token)
        .await
}

#[cfg(test)]
mod tests {
    use crate::api::Vehicle;
    use crate::auth::oauth::OAuthClient;
    use crate::auth::store::TokenStore;
    use crate::config::Config;
    use crate::error::AppError;

    use super::*;

    fn test_config() -> Config {
        Config {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            redirect_uri: "http://localhost:8484/callback".into(),
            audience: "https://fleet-api.prd.na.vn.cloud.tesla.com".into(),
            callback_port: 8484,
            domain: Some("example.com".into()),
        }
    }

    fn test_app() -> App {
        App {
            screen: Screen::Home,
            auth_status: AuthStatus::Authenticated,
            tokens: None,
            status_message: String::new(),
            vehicles: vec![
                Vehicle {
                    id: "1".into(),
                    vin: "5YJSA11111111111".into(),
                    display_name: "Car 1".into(),
                    state: "online".into(),
                    in_service: false,
                },
                Vehicle {
                    id: "2".into(),
                    vin: "5YJSA22222222222".into(),
                    display_name: "Car 2".into(),
                    state: "asleep".into(),
                    in_service: false,
                },
            ],
            vehicles_status: VehiclesStatus::Loaded,
            selected_vehicle: 0,
            config: test_config(),
            oauth: OAuthClient::new(test_config()),
            token_store: TokenStore::with_path(std::env::temp_dir().join("lazytesla-app-test.json")),
            pending_state: None,
        }
    }

    #[test]
    fn apply_vehicles_updates_loaded_state() {
        let mut app = test_app();
        app.apply_vehicles(Ok(vec![Vehicle {
            id: "9".into(),
            vin: "5YJSA99999999999".into(),
            display_name: "Roadster".into(),
            state: "offline".into(),
            in_service: true,
        }]));

        assert_eq!(app.vehicles.len(), 1);
        assert_eq!(app.vehicles_status, VehiclesStatus::Loaded);
        assert_eq!(app.status_message, "Loaded 1 vehicle(s)");
        assert_eq!(app.selected_vehicle, 0);
    }

    #[test]
    fn apply_vehicles_records_errors() {
        let mut app = test_app();
        app.apply_vehicles(Err(AppError::Api("registration required".into())));

        assert!(app.vehicles.is_empty());
        assert_eq!(
            app.vehicles_status,
            VehiclesStatus::Error("API error: registration required".into())
        );
    }

    #[test]
    fn vehicle_selection_wraps_within_bounds() {
        let mut app = test_app();

        app.select_next_vehicle();
        assert_eq!(app.selected_vehicle, 1);

        app.select_next_vehicle();
        assert_eq!(app.selected_vehicle, 1);

        app.select_previous_vehicle();
        assert_eq!(app.selected_vehicle, 0);

        app.select_previous_vehicle();
        assert_eq!(app.selected_vehicle, 0);
    }
}