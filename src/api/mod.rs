mod client;
mod debug_curl;
mod details;
mod service;
mod vehicles;

pub use client::{needs_partner_registration, FleetClient};
pub use details::VehicleDetails;
pub use service::{FleetApi, VehicleRefreshResult};
pub use vehicles::Vehicle;