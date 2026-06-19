mod client;
mod service;
mod vehicles;

pub use client::{needs_partner_registration, FleetClient};
pub use service::FleetApi;
pub use vehicles::Vehicle;