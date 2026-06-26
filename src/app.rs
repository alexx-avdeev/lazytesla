use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::api::{FleetApi, Vehicle, VehicleDetails, VehicleRefreshResult};
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
    pub vehicle_details_cache: HashMap<String, VehicleDetails>,
    pub details_refreshed_at: Option<DateTime<Utc>>,
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
            vehicle_details_cache: HashMap::new(),
            details_refreshed_at: None,
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

    pub fn selected_vehicle(&self) -> Option<&Vehicle> {
        self.vehicles.get(self.selected_vehicle)
    }

    pub fn selected_vehicle_details(&self) -> Option<&VehicleDetails> {
        let vehicle = self.selected_vehicle()?;
        self.vehicle_details_cache.get(&vehicle.vin)
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
        self.status_message = "Signed in successfully. Loading vehicles...".into();
        self.clear_vehicle_data();
        Ok(())
    }

    fn clear_vehicle_data(&mut self) {
        self.vehicles.clear();
        self.vehicle_details_cache.clear();
        self.details_refreshed_at = None;
        self.selected_vehicle = 0;
        self.vehicles_status = VehiclesStatus::Idle;
    }

    pub fn begin_vehicle_refresh(&mut self) {
        self.vehicles_status = VehiclesStatus::Loading;
        self.status_message = "Refreshing vehicles and details...".into();
    }

    pub fn apply_vehicle_refresh(&mut self, result: Result<VehicleRefreshResult>) {
        match result {
            Ok(refresh) => {
                if !self.vehicles.is_empty() {
                    let selected_vin = self
                        .vehicles
                        .get(self.selected_vehicle)
                        .map(|vehicle| vehicle.vin.clone());
                    if let Some(vin) = selected_vin {
                        self.selected_vehicle = refresh
                            .vehicles
                            .iter()
                            .position(|vehicle| vehicle.vin == vin)
                            .unwrap_or(0);
                    }
                } else {
                    self.selected_vehicle = 0;
                }

                self.vehicles = refresh.vehicles;
                self.vehicle_details_cache = refresh.details;
                self.details_refreshed_at = self
                    .vehicle_details_cache
                    .values()
                    .map(|detail| detail.fetched_at)
                    .max();
                self.vehicles_status = VehiclesStatus::Loaded;
                self.status_message = if self.vehicles.is_empty() {
                    "No vehicles found on this account".into()
                } else {
                    format!(
                        "Loaded {} vehicle(s), {} detail cache entries",
                        self.vehicles.len(),
                        self.vehicle_details_cache.len()
                    )
                };
            }
            Err(err) => {
                self.vehicles.clear();
                self.vehicle_details_cache.clear();
                self.details_refreshed_at = None;
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
        self.clear_vehicle_data();
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

pub async fn refresh_vehicles(request: VehicleLoadRequest) -> Result<VehicleRefreshResult> {
    FleetApi::from_config(&request.config)
        .refresh_vehicles(&request.config, &request.access_token)
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
            vehicle_details_cache: HashMap::new(),
            details_refreshed_at: None,
            selected_vehicle: 0,
            config: test_config(),
            oauth: OAuthClient::new(test_config()),
            token_store: TokenStore::with_path(std::env::temp_dir().join("lazytesla-app-test.json")),
            pending_state: None,
        }
    }

    #[test]
    fn apply_vehicle_refresh_updates_cache() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        let details = VehicleDetails {
            vin: "5YJSA99999999999".into(),
            display_name: "Roadster".into(),
            state: "offline".into(),
            in_service: true,
            battery_level: Some(80),
            charging_state: Some("Complete".into()),
            battery_range: Some(250.0),
            charge_limit_soc: Some(90),
            locked: Some(true),
            odometer: Some(12_345.0),
            car_version: Some("2024.1".into()),
            inside_temp: Some(21.0),
            outside_temp: Some(10.0),
            climate_on: Some(false),
            temperature_units: Some("F".into()),
            fetched_at,
        };

        let mut cache = HashMap::new();
        cache.insert(details.vin.clone(), details);

        app.apply_vehicle_refresh(Ok(VehicleRefreshResult {
            vehicles: vec![Vehicle {
                id: "9".into(),
                vin: "5YJSA99999999999".into(),
                display_name: "Roadster".into(),
                state: "offline".into(),
                in_service: true,
            }],
            details: cache,
        }));

        assert_eq!(app.vehicles.len(), 1);
        assert_eq!(app.vehicles_status, VehiclesStatus::Loaded);
        assert!(app.selected_vehicle_details().is_some());
        assert_eq!(app.selected_vehicle_details().unwrap().battery_level, Some(80));
    }

    #[test]
    fn apply_vehicle_refresh_records_errors() {
        let mut app = test_app();
        app.apply_vehicle_refresh(Err(AppError::Api("registration required".into())));

        assert!(app.vehicles.is_empty());
        assert!(app.vehicle_details_cache.is_empty());
        assert_eq!(
            app.vehicles_status,
            VehiclesStatus::Error("API error: registration required".into())
        );
    }

    #[test]
    fn apply_vehicle_refresh_preserves_selected_vehicle_by_vin() {
        let mut app = test_app();
        app.selected_vehicle = 1;

        app.apply_vehicle_refresh(Ok(VehicleRefreshResult {
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
            details: HashMap::new(),
        }));

        assert_eq!(app.selected_vehicle, 1);
        assert_eq!(app.vehicles[1].vin, "5YJSA22222222222");
    }

    #[test]
    fn vehicle_selection_does_not_clear_cache() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: Some(50),
                charging_state: None,
                battery_range: None,
                charge_limit_soc: None,
                locked: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                temperature_units: None,
                fetched_at,
            },
        );

        app.select_next_vehicle();
        assert_eq!(app.selected_vehicle, 1);
        assert_eq!(app.vehicle_details_cache.len(), 1);
    }
}