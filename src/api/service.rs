use std::collections::HashMap;
use std::path::Path;

use crate::api::commands::{ChargeAction, ClimateAction, LockAction, WindowAction};
use serde_json::json;
use crate::api::details::VehicleDetails;
use crate::api::{needs_partner_registration, FleetClient, Vehicle};
use crate::auth::partner::PartnerAuth;
use crate::config::Config;
use crate::error::{AppError, Result};
use crate::vehicle_command::VehicleCommandClient;
use crate::vehicle_command::VehicleCommandError;

pub struct FleetApi {
    fleet: FleetClient,
    command: FleetClient,
    partner: PartnerAuth,
    proxy_configured: bool,
    vcp: Option<VehicleCommandClient>,
}

#[derive(Debug, Clone)]
pub struct VehicleRefreshResult {
    pub vehicles: Vec<Vehicle>,
    pub details: HashMap<String, VehicleDetails>,
}

impl FleetApi {
    pub fn from_config(config: &Config) -> Result<Self> {
        let vcp = if let Some(path) = &config.fleet_key_path {
            Some(
                VehicleCommandClient::new(Path::new(path), &config.audience)
                    .map_err(map_vehicle_command_error)?,
            )
        } else {
            None
        };

        Ok(Self {
            fleet: FleetClient::new(config.audience.clone()),
            command: FleetClient::for_config(config)?,
            partner: PartnerAuth::new(config.clone()),
            proxy_configured: !config.uses_native_commands()
                && config.command_proxy_url.is_some(),
            vcp,
        })
    }

    pub fn with_clients(fleet: FleetClient, command: FleetClient, partner: PartnerAuth) -> Self {
        Self {
            fleet,
            command,
            partner,
            proxy_configured: true,
            vcp: None,
        }
    }

    pub async fn refresh_vehicles(
        &self,
        config: &Config,
        access_token: &str,
    ) -> Result<VehicleRefreshResult> {
        let vehicles = self.fetch_vehicles(config, access_token).await?;
        let mut details = HashMap::new();

        for vehicle in &vehicles {
            match self
                .fleet
                .get_vehicle_data(&vehicle.vin, access_token)
                .await
            {
                Ok(detail) => {
                    details.insert(vehicle.vin.clone(), detail);
                }
                Err(_) => {}
            }
        }

        Ok(VehicleRefreshResult { vehicles, details })
    }

    pub async fn fetch_vehicles(&self, config: &Config, access_token: &str) -> Result<Vec<Vehicle>> {
        if let Some(domain) = &config.domain {
            self.register_partner(domain).await?;
        }

        match self.fleet.list_vehicles(access_token).await {
            Ok(vehicles) => Ok(vehicles),
            Err(err) if needs_partner_registration(&err.to_string()) => {
                let Some(domain) = &config.domain else {
                    return Err(AppError::Config(config.registration_help()));
                };

                self.register_partner(domain).await?;
                self.fleet.list_vehicles(access_token).await
            }
            Err(err) => Err(err),
        }
    }

    pub async fn register_partner(&self, domain: &str) -> Result<()> {
        let partner_token = self.partner.partner_token().await?;
        self.fleet.register_partner(&partner_token, domain).await
    }

    pub async fn send_climate_command(
        &mut self,
        vin: &str,
        action: ClimateAction,
        access_token: &str,
    ) -> Result<()> {
        if let Some(vcp) = &mut self.vcp {
            return match action {
                ClimateAction::Start => vcp.climate_on(vin, access_token).await,
                ClimateAction::Stop => vcp.climate_off(vin, access_token).await,
            }
            .map_err(map_vehicle_command_error);
        }

        self.command
            .send_command(
                vin,
                action.command_name(),
                access_token,
                self.proxy_configured,
            )
            .await
    }

    pub async fn send_climate_temp_command(
        &mut self,
        vin: &str,
        driver_celsius: f64,
        passenger_celsius: f64,
        access_token: &str,
    ) -> Result<()> {
        let driver = driver_celsius as f32;
        let passenger = passenger_celsius as f32;

        if let Some(vcp) = &mut self.vcp {
            return vcp
                .set_climate_temp(vin, access_token, driver, passenger)
                .await
                .map_err(map_vehicle_command_error);
        }

        self.command
            .send_command_with_body(
                vin,
                "set_temps",
                access_token,
                self.proxy_configured,
                json!({
                    "driver_temp": driver_celsius,
                    "passenger_temp": passenger_celsius,
                }),
            )
            .await
    }

    pub async fn send_charge_limit_command(
        &mut self,
        vin: &str,
        percent: u8,
        access_token: &str,
    ) -> Result<()> {
        if let Some(vcp) = &mut self.vcp {
            return vcp
                .set_charge_limit(vin, access_token, percent)
                .await
                .map_err(map_vehicle_command_error);
        }

        self.command
            .send_command_with_body(
                vin,
                "set_charge_limit",
                access_token,
                self.proxy_configured,
                json!({ "percent": percent }),
            )
            .await
    }

    pub async fn send_lock_command(
        &mut self,
        vin: &str,
        action: LockAction,
        access_token: &str,
    ) -> Result<()> {
        if let Some(vcp) = &mut self.vcp {
            return match action {
                LockAction::Lock => vcp.lock(vin, access_token).await,
                LockAction::Unlock => vcp.unlock(vin, access_token).await,
            }
            .map_err(map_vehicle_command_error);
        }

        self.command
            .send_command(
                vin,
                action.command_name(),
                access_token,
                self.proxy_configured,
            )
            .await
    }

    pub async fn send_window_command(
        &mut self,
        vin: &str,
        action: WindowAction,
        access_token: &str,
    ) -> Result<()> {
        if let Some(vcp) = &mut self.vcp {
            return match action {
                WindowAction::Vent => vcp.vent_windows(vin, access_token).await,
                WindowAction::Close => vcp.close_windows(vin, access_token).await,
            }
            .map_err(map_vehicle_command_error);
        }

        self.command
            .send_command_with_body(
                vin,
                action.command_name(),
                access_token,
                self.proxy_configured,
                json!({
                    "command": action.command_body_value(),
                    "lat": 0.0,
                    "lon": 0.0,
                }),
            )
            .await
    }

    pub async fn send_charge_command(
        &mut self,
        vin: &str,
        action: ChargeAction,
        access_token: &str,
    ) -> Result<()> {
        if let Some(vcp) = &mut self.vcp {
            return match action {
                ChargeAction::Start => vcp.charge_start(vin, access_token).await,
                ChargeAction::Stop => vcp.charge_stop(vin, access_token).await,
            }
            .map_err(map_vehicle_command_error);
        }

        self.command
            .send_command(
                vin,
                action.command_name(),
                access_token,
                self.proxy_configured,
            )
            .await
    }
}

fn map_vehicle_command_error(err: VehicleCommandError) -> AppError {
    match err {
        VehicleCommandError::KeyNotPaired => AppError::Config(
            "vehicle does not recognize your fleet key; pair via https://tesla.com/_ak/<your_domain>"
                .into(),
        ),
        VehicleCommandError::VehicleUnavailable(message) => AppError::Api(message),
        VehicleCommandError::InvalidKey(message) => AppError::Config(message),
        other => AppError::Api(other.to_string()),
    }
}