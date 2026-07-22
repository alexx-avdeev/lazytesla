mod charge_limit;
mod client;
mod commands;
mod debug_curl;
mod details;
mod service;
mod temperature;
mod vehicles;

pub use charge_limit::{
    clamp_charge_limit, parse_charge_limit, resolve_charge_limit_bounds, ChargeLimitBounds,
    CHARGE_LIMIT_STEP, MAX_CHARGE_LIMIT_PERCENT, MIN_CHARGE_LIMIT_PERCENT,
};
pub use client::{needs_partner_registration, FleetClient};
pub use commands::{ClimateAction, LockAction};
pub use details::VehicleDetails;
pub use temperature::{
    celsius_to_display, celsius_to_setting_display, clamp_setting_display, display_to_celsius,
    format_temp, parse_display_temperature, resolve_celsius_bounds, round_celsius_for_api,
    temp_adjust_step,
    validate_celsius, CelsiusBounds, MAX_TEMP_CELSIUS, MIN_TEMP_CELSIUS,
};
pub use service::{FleetApi, VehicleRefreshResult};
pub use vehicles::Vehicle;