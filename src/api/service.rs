use std::collections::HashMap;

use crate::api::details::VehicleDetails;
use crate::api::{needs_partner_registration, FleetClient, Vehicle};
use crate::auth::partner::PartnerAuth;
use crate::config::Config;
use crate::error::{AppError, Result};

pub struct FleetApi {
    fleet: FleetClient,
    partner: PartnerAuth,
}

#[derive(Debug, Clone)]
pub struct VehicleRefreshResult {
    pub vehicles: Vec<Vehicle>,
    pub details: HashMap<String, VehicleDetails>,
}

impl FleetApi {
    pub fn from_config(config: &Config) -> Self {
        Self {
            fleet: FleetClient::new(config.audience.clone()),
            partner: PartnerAuth::new(config.clone()),
        }
    }

    pub fn with_clients(fleet: FleetClient, partner: PartnerAuth) -> Self {
        Self { fleet, partner }
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
}