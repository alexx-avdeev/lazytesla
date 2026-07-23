use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::api::{
    celsius_to_setting_display,
    clamp_charge_limit,
    clamp_setting_display,
    format_temp,
    parse_charge_limit,
    parse_display_temperature,
    round_celsius_for_api,
    ChargeAction,
    ClimateAction,
    FleetApi,
    LockAction,
    Vehicle,
    VehicleDetails,
    VehicleRefreshResult,
    WindowAction,
};
use crate::help_menu::{HelpEntry, HomeCommand, HELP_ENTRIES};
use crate::auth::oauth::{OAuthClient, TokenSet};
use crate::auth::store::{StoredTokens, TokenStore};
use crate::config::Config;
use crate::error::{AppError, Result};
use crate::store::{StoredVehicleCache, VehicleStore};

const LOGIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
const SPINNER_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

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
    pub refresh_spinner_frame: usize,
    pub temp_input: Option<String>,
    pub charge_limit_input: Option<String>,
    /// Selected index in the `?` help modal; `None` when closed.
    pub help_selection: Option<usize>,
    pending_commands: u32,
    config: Config,
    oauth: OAuthClient,
    token_store: TokenStore,
    vehicle_store: VehicleStore,
    pending_state: Option<String>,
}

impl App {
    pub async fn new(config: Config) -> Result<Self> {
        let oauth = OAuthClient::new(config.clone());
        let token_store = TokenStore::new()?;
        let vehicle_store = VehicleStore::new()?;

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
            refresh_spinner_frame: 0,
            temp_input: None,
            charge_limit_input: None,
            help_selection: None,
            pending_commands: 0,
            config,
            oauth,
            token_store,
            vehicle_store,
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
        self.load_cached_vehicles()?;
        self.status_message = if self.vehicles.is_empty() {
            "Signed in successfully. Loading vehicles...".into()
        } else {
            format!(
                "Signed in. Showing {} cached vehicle(s). Refreshing...",
                self.vehicles.len()
            )
        };
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
        if matches!(self.vehicles_status, VehiclesStatus::Loading) {
            return;
        }
        self.vehicles_status = VehiclesStatus::Loading;
        self.refresh_spinner_frame = 0;
        self.status_message = "Refreshing vehicles and details...".into();
    }

    pub fn tick_spinner(&mut self) {
        if self.shows_spinner() {
            self.refresh_spinner_frame = (self.refresh_spinner_frame + 1) % SPINNER_FRAMES.len();
        }
    }

    pub fn refresh_spinner(&self) -> &'static str {
        SPINNER_FRAMES[self.refresh_spinner_frame % SPINNER_FRAMES.len()]
    }

    pub fn shows_spinner(&self) -> bool {
        self.is_refreshing() || self.pending_commands > 0
    }

    pub fn is_refreshing(&self) -> bool {
        matches!(self.vehicles_status, VehiclesStatus::Loading)
    }

    fn begin_async_command(&mut self) {
        self.pending_commands += 1;
        self.refresh_spinner_frame = 0;
    }

    fn end_async_command(&mut self) {
        self.pending_commands = self.pending_commands.saturating_sub(1);
    }

    pub fn has_cached_vehicles(&self) -> bool {
        !self.vehicles.is_empty()
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
                let _ = self.save_cached_vehicles();
            }
            Err(err) => {
                if self.vehicles.is_empty() {
                    self.vehicles_status = VehiclesStatus::Error(err.to_string());
                } else {
                    self.vehicles_status = VehiclesStatus::Loaded;
                }
                self.status_message = format!("Refresh failed: {err}");
            }
        }
    }

    fn load_cached_vehicles(&mut self) -> Result<()> {
        let Some(cache) = self.vehicle_store.load()? else {
            return Ok(());
        };

        self.vehicles = cache.vehicles;
        self.vehicle_details_cache = cache.details;
        self.details_refreshed_at = cache.details_refreshed_at;
        self.selected_vehicle = cache
            .selected_vehicle
            .min(self.vehicles.len().saturating_sub(1));
        if !self.vehicles.is_empty() {
            self.vehicles_status = VehiclesStatus::Loaded;
        }
        Ok(())
    }

    fn save_cached_vehicles(&self) -> Result<()> {
        if self.vehicles.is_empty() {
            return Ok(());
        }

        self.vehicle_store.save(&StoredVehicleCache {
            vehicles: self.vehicles.clone(),
            details: self.vehicle_details_cache.clone(),
            selected_vehicle: self.selected_vehicle,
            details_refreshed_at: self.details_refreshed_at,
            saved_at: Utc::now(),
        })
    }

    pub fn vehicle_load_request(&self) -> Option<VehicleLoadRequest> {
        let access_token = self.tokens.as_ref()?.access_token.clone();
        Some(VehicleLoadRequest {
            config: self.config.clone(),
            access_token,
        })
    }

    pub fn climate_toggle_request(&self) -> Option<ClimateCommandRequest> {
        let vehicle = self.selected_vehicle()?;
        let access_token = self.tokens.as_ref()?.access_token.clone();
        let climate_on = self
            .selected_vehicle_details()
            .and_then(|details| details.climate_on);
        let action = ClimateAction::from_climate_on(climate_on);

        Some(ClimateCommandRequest {
            config: self.config.clone(),
            access_token,
            vin: vehicle.vin.clone(),
            action,
        })
    }

    pub fn begin_climate_command(&mut self, action: ClimateAction) {
        self.begin_async_command();
        self.status_message = match action {
            ClimateAction::Start => "Turning climate on...".into(),
            ClimateAction::Stop => "Turning climate off...".into(),
        };
    }

    pub fn apply_climate_command(&mut self, vin: &str, result: Result<ClimateAction>) {
        self.end_async_command();
        match result {
            Ok(action) => {
                if let Some(details) = self.vehicle_details_cache.get_mut(vin) {
                    details.climate_on = Some(action.climate_on());
                }
                self.status_message = match action {
                    ClimateAction::Start => "Climate turned on".into(),
                    ClimateAction::Stop => "Climate turned off".into(),
                };
                let _ = self.save_cached_vehicles();
            }
            Err(err) => {
                self.status_message = err.to_string();
            }
        }
    }

    pub fn lock_toggle_request(&self) -> Option<LockCommandRequest> {
        let vehicle = self.selected_vehicle()?;
        let access_token = self.tokens.as_ref()?.access_token.clone();
        let locked = self
            .selected_vehicle_details()
            .and_then(|details| details.locked);
        let action = LockAction::from_locked(locked);

        Some(LockCommandRequest {
            config: self.config.clone(),
            access_token,
            vin: vehicle.vin.clone(),
            action,
        })
    }

    pub fn begin_lock_command(&mut self, action: LockAction) {
        self.begin_async_command();
        self.status_message = match action {
            LockAction::Lock => "Locking vehicle...".into(),
            LockAction::Unlock => "Unlocking vehicle...".into(),
        };
    }

    pub fn window_toggle_request(&self) -> Option<WindowCommandRequest> {
        if self.is_modal_open() {
            return None;
        }

        let vehicle = self.selected_vehicle()?;
        let access_token = self.tokens.as_ref()?.access_token.clone();
        let windows_open = self
            .selected_vehicle_details()
            .and_then(|details| details.any_window_open());
        let action = WindowAction::from_windows_open(windows_open);

        Some(WindowCommandRequest {
            config: self.config.clone(),
            access_token,
            vin: vehicle.vin.clone(),
            action,
        })
    }

    pub fn begin_window_command(&mut self, action: WindowAction) {
        self.begin_async_command();
        self.status_message = match action {
            WindowAction::Vent => "Venting windows...".into(),
            WindowAction::Close => "Closing windows...".into(),
        };
    }

    pub fn apply_window_command(&mut self, vin: &str, result: Result<WindowAction>) {
        self.end_async_command();
        match result {
            Ok(action) => {
                if let Some(details) = self.vehicle_details_cache.get_mut(vin) {
                    details.set_windows_open_state(action.windows_open());
                }
                self.status_message = match action {
                    WindowAction::Vent => "Windows vented".into(),
                    WindowAction::Close => "Windows closed".into(),
                };
                let _ = self.save_cached_vehicles();
            }
            Err(err) => {
                self.status_message = err.to_string();
            }
        }
    }

    pub fn charge_toggle_request(&self) -> Option<ChargeCommandRequest> {
        if self.is_modal_open() {
            return None;
        }

        let vehicle = self.selected_vehicle()?;
        let access_token = self.tokens.as_ref()?.access_token.clone();
        let charging_state = self
            .selected_vehicle_details()
            .and_then(|details| details.charging_state.as_deref());
        let action = ChargeAction::from_charging_state(charging_state);

        Some(ChargeCommandRequest {
            config: self.config.clone(),
            access_token,
            vin: vehicle.vin.clone(),
            action,
        })
    }

    pub fn begin_charge_command(&mut self, action: ChargeAction) {
        self.begin_async_command();
        self.status_message = match action {
            ChargeAction::Start => "Starting charge...".into(),
            ChargeAction::Stop => "Stopping charge...".into(),
        };
    }

    pub fn apply_charge_command(&mut self, vin: &str, result: Result<ChargeAction>) {
        self.end_async_command();
        match result {
            Ok(action) => {
                if let Some(details) = self.vehicle_details_cache.get_mut(vin) {
                    details.charging_state = Some(action.resulting_charging_state().into());
                }
                self.status_message = match action {
                    ChargeAction::Start => "Charging started".into(),
                    ChargeAction::Stop => "Charging stopped".into(),
                };
                let _ = self.save_cached_vehicles();
            }
            Err(err) => {
                self.status_message = err.to_string();
            }
        }
    }

    pub fn is_editing_temp(&self) -> bool {
        self.temp_input.is_some()
    }

    pub fn is_editing_charge_limit(&self) -> bool {
        self.charge_limit_input.is_some()
    }

    pub fn is_help_open(&self) -> bool {
        self.help_selection.is_some()
    }

    pub fn is_modal_open(&self) -> bool {
        self.is_editing_temp() || self.is_editing_charge_limit() || self.is_help_open()
    }

    pub fn open_help(&mut self) {
        if self.is_modal_open() {
            return;
        }
        self.help_selection = Some(0);
        self.status_message.clear();
    }

    pub fn close_help(&mut self) {
        self.help_selection = None;
        self.status_message = "Help closed".into();
    }

    pub fn help_move_selection(&mut self, delta: i32) {
        let Some(selected) = self.help_selection.as_mut() else {
            return;
        };
        let len = HELP_ENTRIES.len() as i32;
        if len == 0 {
            return;
        }
        let next = (*selected as i32 + delta).rem_euclid(len);
        *selected = next as usize;
    }

    pub fn help_entries(&self) -> &'static [HelpEntry] {
        HELP_ENTRIES
    }

    pub fn help_selected_index(&self) -> Option<usize> {
        self.help_selection
    }

    /// Closes help and returns the selected command to run.
    pub fn confirm_help_selection(&mut self) -> Option<HomeCommand> {
        let index = self.help_selection.take()?;
        HELP_ENTRIES.get(index).map(|entry| entry.command)
    }

    pub fn begin_temp_input(&mut self) {
        if self.is_modal_open() {
            return;
        }

        let Some(details) = self.selected_vehicle_details().cloned() else {
            self.status_message = "No vehicle details available. Press r to refresh.".into();
            return;
        };

        let gui_units = details.temperature_units.as_deref();
        let default_display = details
            .target_temp_setting()
            .unwrap_or_else(|| celsius_to_setting_display(22.0, gui_units));

        self.temp_input = Some(format_temp(default_display));
        // Modal shows the editor UI; keep status clear until validation fails or send starts.
        self.status_message.clear();
    }

    pub fn begin_charge_limit_input(&mut self) {
        if self.is_modal_open() {
            return;
        }

        let Some(details) = self.selected_vehicle_details().cloned() else {
            self.status_message = "No vehicle details available. Press r to refresh.".into();
            return;
        };

        let default = details.charge_limit_soc.unwrap_or(80);
        self.charge_limit_input = Some(default.to_string());
        self.status_message.clear();
    }

    pub fn cancel_temp_input(&mut self) {
        self.temp_input = None;
        self.status_message = "Temperature edit cancelled".into();
    }

    pub fn append_temp_input(&mut self, ch: char) {
        let Some(buffer) = self.temp_input.as_mut() else {
            return;
        };

        if ch.is_ascii_digit() {
            if buffer.len() >= 5 {
                return;
            }
            buffer.push(ch);
            self.status_message.clear();
            return;
        }

        if ch == '.' && !buffer.contains('.') && buffer.len() < 5 {
            if buffer.is_empty() {
                buffer.push('0');
            }
            buffer.push('.');
            self.status_message.clear();
        }
    }

    pub fn backspace_temp_input(&mut self) {
        if let Some(buffer) = self.temp_input.as_mut() {
            buffer.pop();
            self.status_message.clear();
        }
    }

    pub fn adjust_temp_input(&mut self, delta: f64) {
        let Some(details) = self.selected_vehicle_details() else {
            return;
        };

        let gui_units = details.temperature_units.as_deref();
        let bounds = details.temp_bounds();
        let current = self
            .temp_input
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .or_else(|| details.target_temp_setting())
            .unwrap_or_else(|| celsius_to_setting_display(22.0, gui_units));
        let adjusted = clamp_setting_display(current + delta, bounds, gui_units);

        self.temp_input = Some(format_temp(adjusted));
        self.status_message.clear();
    }

    pub fn submit_temp_input(&mut self) -> Option<SetClimateTempRequest> {
        let buffer = self.temp_input.take()?;
        let vin = self.selected_vehicle()?.vin.clone();
        let details = self.selected_vehicle_details()?.clone();
        let access_token = self.tokens.as_ref()?.access_token.clone();
        let gui_units = details.temperature_units.as_deref();

        let bounds = details.temp_bounds();
        let celsius = match parse_display_temperature(&buffer, gui_units, bounds) {
            Ok(value) => round_celsius_for_api(value),
            Err(err) => {
                self.temp_input = Some(buffer);
                self.status_message = err;
                return None;
            }
        };

        let display_temp = celsius_to_setting_display(celsius, gui_units);
        let temperature_unit = details.display_temperature_unit().to_string();

        self.begin_async_command();
        self.status_message = format!(
            "Setting climate to {}°{temperature_unit}...",
            format_temp(display_temp),
        );

        Some(SetClimateTempRequest {
            config: self.config.clone(),
            access_token,
            vin,
            driver_celsius: celsius,
            passenger_celsius: celsius,
            display_temp,
            temperature_unit,
        })
    }

    pub fn apply_set_climate_temp(&mut self, vin: &str, result: Result<SetClimateTempOutcome>) {
        self.end_async_command();
        match result {
            Ok(outcome) => {
                if let Some(details) = self.vehicle_details_cache.get_mut(vin) {
                    details.driver_temp_setting = Some(outcome.display_temp);
                    details.passenger_temp_setting = Some(outcome.display_temp);
                }
                self.status_message = format!(
                    "Climate target set to {}°{}",
                    format_temp(outcome.display_temp),
                    outcome.temperature_unit
                );
                let _ = self.save_cached_vehicles();
            }
            Err(err) => {
                self.status_message = err.to_string();
            }
        }
    }

    pub fn cancel_charge_limit_input(&mut self) {
        self.charge_limit_input = None;
        self.status_message = "Charge limit edit cancelled".into();
    }

    pub fn append_charge_limit_input(&mut self, ch: char) {
        let Some(buffer) = self.charge_limit_input.as_mut() else {
            return;
        };

        if !ch.is_ascii_digit() || buffer.len() >= 3 {
            return;
        }
        buffer.push(ch);
        self.status_message.clear();
    }

    pub fn backspace_charge_limit_input(&mut self) {
        if let Some(buffer) = self.charge_limit_input.as_mut() {
            buffer.pop();
            self.status_message.clear();
        }
    }

    pub fn adjust_charge_limit_input(&mut self, delta: i16) {
        let Some(details) = self.selected_vehicle_details() else {
            return;
        };

        let bounds = details.charge_limit_bounds();
        let current = self
            .charge_limit_input
            .as_deref()
            .and_then(|value| value.parse::<i16>().ok())
            .or_else(|| details.charge_limit_soc.map(i16::from))
            .unwrap_or(80);
        let adjusted = clamp_charge_limit(current + delta, bounds);

        self.charge_limit_input = Some(adjusted.to_string());
        self.status_message.clear();
    }

    pub fn submit_charge_limit_input(&mut self) -> Option<SetChargeLimitRequest> {
        let buffer = self.charge_limit_input.take()?;
        let vin = self.selected_vehicle()?.vin.clone();
        let details = self.selected_vehicle_details()?.clone();
        let access_token = self.tokens.as_ref()?.access_token.clone();

        let bounds = details.charge_limit_bounds();
        let percent = match parse_charge_limit(&buffer, bounds) {
            Ok(value) => value,
            Err(err) => {
                self.charge_limit_input = Some(buffer);
                self.status_message = err;
                return None;
            }
        };

        if details.charge_limit_soc == Some(percent) {
            self.status_message = format!("Charge limit already {percent}%");
            return None;
        }

        self.begin_async_command();
        self.status_message = format!("Setting charge limit to {percent}%...");

        Some(SetChargeLimitRequest {
            config: self.config.clone(),
            access_token,
            vin,
            percent,
        })
    }

    pub fn apply_set_charge_limit(&mut self, vin: &str, result: Result<SetChargeLimitOutcome>) {
        self.end_async_command();
        match result {
            Ok(outcome) => {
                if let Some(details) = self.vehicle_details_cache.get_mut(vin) {
                    details.charge_limit_soc = Some(outcome.percent);
                }
                self.status_message = format!("Charge limit set to {}%", outcome.percent);
                let _ = self.save_cached_vehicles();
            }
            Err(err) => {
                self.status_message = err.to_string();
            }
        }
    }

    pub fn apply_lock_command(&mut self, vin: &str, result: Result<LockAction>) {
        self.end_async_command();
        match result {
            Ok(action) => {
                if let Some(details) = self.vehicle_details_cache.get_mut(vin) {
                    details.locked = Some(action.locked());
                }
                self.status_message = match action {
                    LockAction::Lock => "Vehicle locked".into(),
                    LockAction::Unlock => "Vehicle unlocked".into(),
                };
                let _ = self.save_cached_vehicles();
            }
            Err(err) => {
                self.status_message = err.to_string();
            }
        }
    }

    pub fn select_previous_vehicle(&mut self) {
        if self.vehicles.is_empty() {
            return;
        }
        self.selected_vehicle = self.selected_vehicle.saturating_sub(1);
        let _ = self.save_cached_vehicles();
    }

    pub fn select_next_vehicle(&mut self) {
        if self.vehicles.is_empty() {
            return;
        }
        let last = self.vehicles.len().saturating_sub(1);
        self.selected_vehicle = (self.selected_vehicle + 1).min(last);
        let _ = self.save_cached_vehicles();
    }

    pub fn logout(&mut self) -> Result<()> {
        self.token_store.clear()?;
        self.vehicle_store.clear()?;
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
    let api = FleetApi::from_config(&request.config)?;
    api.refresh_vehicles(&request.config, &request.access_token)
        .await
}

#[derive(Debug, Clone)]
pub struct ClimateCommandRequest {
    pub config: Config,
    pub access_token: String,
    pub vin: String,
    pub action: ClimateAction,
}

#[derive(Debug)]
pub struct ClimateCommandOutcome {
    pub vin: String,
    pub result: Result<ClimateAction>,
}

pub async fn send_climate_command(request: ClimateCommandRequest) -> ClimateCommandOutcome {
    let vin = request.vin.clone();
    let action = request.action;
    let result = match FleetApi::from_config(&request.config) {
        Ok(mut api) => api
            .send_climate_command(&request.vin, action, &request.access_token)
            .await
            .map(|()| action),
        Err(err) => Err(err),
    };

    ClimateCommandOutcome { vin, result }
}

#[derive(Debug, Clone)]
pub struct LockCommandRequest {
    pub config: Config,
    pub access_token: String,
    pub vin: String,
    pub action: LockAction,
}

#[derive(Debug)]
pub struct LockCommandOutcome {
    pub vin: String,
    pub result: Result<LockAction>,
}

#[derive(Debug, Clone)]
pub struct SetClimateTempRequest {
    pub config: Config,
    pub access_token: String,
    pub vin: String,
    pub driver_celsius: f64,
    pub passenger_celsius: f64,
    pub display_temp: f64,
    pub temperature_unit: String,
}

#[derive(Debug, Clone)]
pub struct SetClimateTempOutcome {
    pub display_temp: f64,
    pub temperature_unit: String,
}

#[derive(Debug)]
pub struct SetClimateTempCommandOutcome {
    pub vin: String,
    pub result: Result<SetClimateTempOutcome>,
}

pub async fn send_set_climate_temp_command(
    request: SetClimateTempRequest,
) -> SetClimateTempCommandOutcome {
    let vin = request.vin.clone();
    let display_temp = request.display_temp;
    let temperature_unit = request.temperature_unit.clone();
    let result = match FleetApi::from_config(&request.config) {
        Ok(mut api) => api
            .send_climate_temp_command(
                &request.vin,
                request.driver_celsius,
                request.passenger_celsius,
                &request.access_token,
            )
            .await
            .map(|()| SetClimateTempOutcome {
                display_temp,
                temperature_unit,
            }),
        Err(err) => Err(err),
    };

    SetClimateTempCommandOutcome { vin, result }
}

#[derive(Debug, Clone)]
pub struct SetChargeLimitRequest {
    pub config: Config,
    pub access_token: String,
    pub vin: String,
    pub percent: u8,
}

#[derive(Debug, Clone)]
pub struct SetChargeLimitOutcome {
    pub percent: u8,
}

#[derive(Debug)]
pub struct SetChargeLimitCommandOutcome {
    pub vin: String,
    pub result: Result<SetChargeLimitOutcome>,
}

pub async fn send_set_charge_limit_command(
    request: SetChargeLimitRequest,
) -> SetChargeLimitCommandOutcome {
    let vin = request.vin.clone();
    let percent = request.percent;
    let result = match FleetApi::from_config(&request.config) {
        Ok(mut api) => api
            .send_charge_limit_command(&request.vin, percent, &request.access_token)
            .await
            .map(|()| SetChargeLimitOutcome { percent }),
        Err(err) => Err(err),
    };

    SetChargeLimitCommandOutcome { vin, result }
}

pub async fn send_lock_command(request: LockCommandRequest) -> LockCommandOutcome {
    let vin = request.vin.clone();
    let action = request.action;
    let result = match FleetApi::from_config(&request.config) {
        Ok(mut api) => api
            .send_lock_command(&request.vin, action, &request.access_token)
            .await
            .map(|()| action),
        Err(err) => Err(err),
    };

    LockCommandOutcome { vin, result }
}

#[derive(Debug, Clone)]
pub struct WindowCommandRequest {
    pub config: Config,
    pub access_token: String,
    pub vin: String,
    pub action: WindowAction,
}

#[derive(Debug)]
pub struct WindowCommandOutcome {
    pub vin: String,
    pub result: Result<WindowAction>,
}

pub async fn send_window_command(request: WindowCommandRequest) -> WindowCommandOutcome {
    let vin = request.vin.clone();
    let action = request.action;
    let result = match FleetApi::from_config(&request.config) {
        Ok(mut api) => api
            .send_window_command(&request.vin, action, &request.access_token)
            .await
            .map(|()| action),
        Err(err) => Err(err),
    };

    WindowCommandOutcome { vin, result }
}

#[derive(Debug, Clone)]
pub struct ChargeCommandRequest {
    pub config: Config,
    pub access_token: String,
    pub vin: String,
    pub action: ChargeAction,
}

#[derive(Debug)]
pub struct ChargeCommandOutcome {
    pub vin: String,
    pub result: Result<ChargeAction>,
}

pub async fn send_charge_command(request: ChargeCommandRequest) -> ChargeCommandOutcome {
    let vin = request.vin.clone();
    let action = request.action;
    let result = match FleetApi::from_config(&request.config) {
        Ok(mut api) => api
            .send_charge_command(&request.vin, action, &request.access_token)
            .await
            .map(|()| action),
        Err(err) => Err(err),
    };

    ChargeCommandOutcome { vin, result }
}

#[cfg(test)]
mod tests {
    use crate::api::Vehicle;
    use crate::auth::oauth::OAuthClient;
    use crate::auth::store::{StoredTokens, TokenStore};
    use crate::config::Config;
    use crate::error::AppError;
    use crate::store::VehicleStore;

    use super::*;

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

    fn test_app() -> App {
        App {
            screen: Screen::Home,
            auth_status: AuthStatus::Authenticated,
            tokens: Some(StoredTokens {
                access_token: "test-token".into(),
                refresh_token: "test-refresh".into(),
                expires_at: Utc::now() + chrono::Duration::hours(1),
            }),
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
            refresh_spinner_frame: 0,
            temp_input: None,
            charge_limit_input: None,
            help_selection: None,
            pending_commands: 0,
            config: test_config(),
            oauth: OAuthClient::new(test_config()),
            token_store: TokenStore::with_path(std::env::temp_dir().join("lazytesla-app-test.json")),
            vehicle_store: VehicleStore::with_path(
                std::env::temp_dir().join(format!("lazytesla-vehicles-app-test-{}", std::process::id())),
            ),
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
            charge_rate: None,
            charge_limit_soc: Some(90),
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
            locked: Some(true),
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
            odometer: Some(12_345.0),
            car_version: Some("2024.1".into()),
            inside_temp: Some(21.0),
            outside_temp: Some(10.0),
            climate_on: Some(false),
            driver_temp_setting: None,
            passenger_temp_setting: None,
            min_avail_temp_celsius: None,
            max_avail_temp_celsius: None,
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

        assert_eq!(app.vehicles.len(), 2);
        assert_eq!(app.vehicles_status, VehiclesStatus::Loaded);
        assert!(app.status_message.contains("Refresh failed"));
    }

    #[test]
    fn apply_vehicle_refresh_error_without_cache_sets_error_state() {
        let mut app = test_app();
        app.vehicles.clear();
        app.apply_vehicle_refresh(Err(AppError::Api("registration required".into())));

        assert!(app.vehicles.is_empty());
        assert_eq!(
            app.vehicles_status,
            VehiclesStatus::Error("API error: registration required".into())
        );
    }

    #[test]
    fn begin_vehicle_refresh_keeps_loaded_vehicles_visible() {
        let mut app = test_app();
        app.begin_vehicle_refresh();

        assert_eq!(app.vehicles.len(), 2);
        assert_eq!(app.vehicles_status, VehiclesStatus::Loading);
        assert!(app.shows_spinner());
    }

    #[test]
    fn begin_climate_command_shows_spinner() {
        let mut app = test_app();
        app.begin_climate_command(ClimateAction::Start);

        assert!(app.shows_spinner());
        assert_eq!(app.status_message, "Turning climate on...");
    }

    #[test]
    fn apply_climate_command_hides_spinner() {
        let mut app = test_app();
        app.begin_climate_command(ClimateAction::Start);
        app.apply_climate_command("5YJSA11111111111", Ok(ClimateAction::Start));

        assert!(!app.shows_spinner());
    }

    #[test]
    fn begin_lock_command_shows_spinner() {
        let mut app = test_app();
        app.begin_lock_command(LockAction::Lock);

        assert!(app.shows_spinner());
        assert_eq!(app.status_message, "Locking vehicle...");
    }

    #[test]
    fn climate_toggle_request_starts_when_climate_off() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: Some(false),
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        let request = app.climate_toggle_request().expect("request should exist");

        assert_eq!(request.action, ClimateAction::Start);
        assert_eq!(request.vin, "5YJSA11111111111");
    }

    #[test]
    fn climate_toggle_request_stops_when_climate_on() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: Some(true),
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        let request = app.climate_toggle_request().expect("request should exist");

        assert_eq!(request.action, ClimateAction::Stop);
    }

    #[test]
    fn lock_toggle_request_locks_when_unlocked() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: Some(false),
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        let request = app.lock_toggle_request().expect("request should exist");

        assert_eq!(request.action, LockAction::Lock);
        assert_eq!(request.vin, "5YJSA11111111111");
    }

    #[test]
    fn lock_toggle_request_unlocks_when_locked() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: Some(true),
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        let request = app.lock_toggle_request().expect("request should exist");

        assert_eq!(request.action, LockAction::Unlock);
    }

    #[test]
    fn apply_lock_command_updates_cached_state() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: Some(false),
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        app.apply_lock_command("5YJSA11111111111", Ok(LockAction::Lock));

        assert_eq!(
            app.vehicle_details_cache
                .get("5YJSA11111111111")
                .unwrap()
                .locked,
            Some(true)
        );
        assert_eq!(app.status_message, "Vehicle locked");
    }

    #[test]
    fn window_toggle_request_vents_when_closed() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
                charge_limit_soc_min: None,
                charge_limit_soc_max: None,
                locked: None,
                fd_window: Some(0),
                fp_window: Some(0),
                rd_window: Some(0),
                rp_window: Some(0),
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        let request = app.window_toggle_request().expect("request");
        assert_eq!(request.action, WindowAction::Vent);
    }

    #[test]
    fn apply_window_command_updates_cached_state() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
                charge_limit_soc_min: None,
                charge_limit_soc_max: None,
                locked: None,
                fd_window: Some(0),
                fp_window: Some(0),
                rd_window: Some(0),
                rp_window: Some(0),
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        app.apply_window_command("5YJSA11111111111", Ok(WindowAction::Vent));

        let details = app.vehicle_details_cache.get("5YJSA11111111111").unwrap();
        assert_eq!(details.any_window_open(), Some(true));
        assert_eq!(app.status_message, "Windows vented");
    }

    #[test]
    fn charge_toggle_request_stops_when_charging() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: Some(40),
                charging_state: Some("Charging".into()),
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: Some(80),
                charge_limit_soc_min: None,
                charge_limit_soc_max: None,
                locked: None,
                fd_window: None,
                fp_window: None,
                rd_window: None,
                rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        let request = app.charge_toggle_request().expect("request");
        assert_eq!(request.action, ChargeAction::Stop);
    }

    #[test]
    fn help_confirm_returns_selected_command() {
        let mut app = test_app();
        app.open_help();
        assert!(app.is_help_open());
        app.help_move_selection(2); // refresh
        let command = app.confirm_help_selection().expect("command");
        assert_eq!(command, HomeCommand::Refresh);
        assert!(!app.is_help_open());
    }

    #[test]
    fn begin_temp_input_seeds_from_target_setting() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: Some(true),
                driver_temp_setting: Some(72.0),
                passenger_temp_setting: Some(72.0),
                min_avail_temp_celsius: Some(15.0),
                max_avail_temp_celsius: Some(28.0),
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );

        app.begin_temp_input();

        assert_eq!(app.temp_input.as_deref(), Some("72"));
        assert!(app.is_editing_temp());
        assert!(app.status_message.is_empty());
    }

    #[test]
    fn submit_temp_input_converts_fahrenheit_to_celsius_for_api() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: Some(true),
                driver_temp_setting: Some(72.0),
                passenger_temp_setting: None,
                min_avail_temp_celsius: Some(15.0),
                max_avail_temp_celsius: Some(28.0),
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );
        app.temp_input = Some("72".into());

        let request = app.submit_temp_input().expect("request");

        assert!((request.driver_celsius - 22.0).abs() < f64::EPSILON);
        assert_eq!(request.display_temp, 72.0);
        assert_eq!(request.temperature_unit, "F");
        assert!(app.shows_spinner());
    }

    #[test]
    fn submit_temp_input_rejects_below_vehicle_minimum() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: Some(true),
                driver_temp_setting: Some(72.0),
                passenger_temp_setting: None,
                min_avail_temp_celsius: Some(15.0),
                max_avail_temp_celsius: Some(28.0),
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );
        app.temp_input = Some("58".into());

        assert!(app.submit_temp_input().is_none());
        assert_eq!(app.temp_input.as_deref(), Some("58"));
        assert!(app.status_message.contains("59°F"));
    }

    #[test]
    fn adjust_temp_input_clamps_to_vehicle_bounds() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: Some(true),
                driver_temp_setting: Some(82.0),
                passenger_temp_setting: None,
                min_avail_temp_celsius: Some(15.0),
                max_avail_temp_celsius: Some(28.0),
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );
        app.temp_input = Some("82".into());

        app.adjust_temp_input(1.0);

        assert_eq!(app.temp_input.as_deref(), Some("82"));
    }

    #[test]
    fn begin_charge_limit_input_seeds_from_cached_limit() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: Some(55),
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: Some(90),
                charge_limit_soc_min: Some(50),
                charge_limit_soc_max: Some(100),
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );

        app.begin_charge_limit_input();

        assert_eq!(app.charge_limit_input.as_deref(), Some("90"));
        assert!(app.is_editing_charge_limit());
    }

    #[test]
    fn submit_charge_limit_rejects_below_minimum() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: Some(80),
                charge_limit_soc_min: Some(50),
                charge_limit_soc_max: Some(100),
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );
        app.charge_limit_input = Some("40".into());

        assert!(app.submit_charge_limit_input().is_none());
        assert_eq!(app.charge_limit_input.as_deref(), Some("40"));
        assert!(app.status_message.contains("50%"));
    }

    #[test]
    fn submit_charge_limit_skips_when_unchanged() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: Some(90),
                charge_limit_soc_min: Some(50),
                charge_limit_soc_max: Some(100),
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );
        app.charge_limit_input = Some("90".into());

        assert!(app.submit_charge_limit_input().is_none());
        assert!(app.charge_limit_input.is_none());
        assert!(!app.is_editing_charge_limit());
        assert_eq!(app.status_message, "Charge limit already 90%");
        assert!(!app.shows_spinner());
    }

    #[test]
    fn adjust_charge_limit_clamps_to_bounds() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: Some(100),
                charge_limit_soc_min: Some(50),
                charge_limit_soc_max: Some(100),
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );
        app.charge_limit_input = Some("100".into());

        app.adjust_charge_limit_input(1);

        assert_eq!(app.charge_limit_input.as_deref(), Some("100"));
    }

    #[test]
    fn apply_set_charge_limit_updates_cached_limit() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: Some(80),
                charge_limit_soc_min: Some(50),
                charge_limit_soc_max: Some(100),
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );

        app.apply_set_charge_limit(
            "5YJSA11111111111",
            Ok(SetChargeLimitOutcome { percent: 70 }),
        );

        assert_eq!(
            app.vehicle_details_cache["5YJSA11111111111"].charge_limit_soc,
            Some(70)
        );
        assert_eq!(app.status_message, "Charge limit set to 70%");
    }

    #[test]
    fn apply_set_climate_temp_updates_cached_target() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
                charge_limit_soc_min: None,
                charge_limit_soc_max: None,
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: Some(true),
                driver_temp_setting: Some(70.0),
                passenger_temp_setting: Some(70.0),
                min_avail_temp_celsius: Some(15.0),
                max_avail_temp_celsius: Some(28.0),
                temperature_units: Some("F".into()),
                fetched_at,
            },
        );

        app.apply_set_climate_temp(
            "5YJSA11111111111",
            Ok(SetClimateTempOutcome {
                display_temp: 72.0,
                temperature_unit: "F".into(),
            }),
        );

        let details = app
            .vehicle_details_cache
            .get("5YJSA11111111111")
            .expect("details");
        assert_eq!(details.driver_temp_setting, Some(72.0));
        assert_eq!(details.passenger_temp_setting, Some(72.0));
        assert_eq!(app.status_message, "Climate target set to 72°F");
    }

    #[test]
    fn apply_climate_command_updates_cached_state() {
        let mut app = test_app();
        let fetched_at = Utc::now();
        app.vehicle_details_cache.insert(
            "5YJSA11111111111".into(),
            VehicleDetails {
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
                battery_level: None,
                charging_state: None,
                battery_range: None,
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: Some(false),
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        app.apply_climate_command("5YJSA11111111111", Ok(ClimateAction::Start));

        assert_eq!(
            app.vehicle_details_cache
                .get("5YJSA11111111111")
                .unwrap()
                .climate_on,
            Some(true)
        );
        assert_eq!(app.status_message, "Climate turned on");
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
            charge_rate: None,
                charge_limit_soc: None,
            charge_limit_soc_min: None,
            charge_limit_soc_max: None,
                locked: None,
            fd_window: None,
            fp_window: None,
            rd_window: None,
            rp_window: None,
                odometer: None,
                car_version: None,
                inside_temp: None,
                outside_temp: None,
                climate_on: None,
                driver_temp_setting: None,
                passenger_temp_setting: None,
                min_avail_temp_celsius: None,
                max_avail_temp_celsius: None,
                temperature_units: None,
                fetched_at,
            },
        );

        app.select_next_vehicle();
        assert_eq!(app.selected_vehicle, 1);
        assert_eq!(app.vehicle_details_cache.len(), 1);
    }
}