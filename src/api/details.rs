use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::temperature;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleDetails {
    pub vin: String,
    pub display_name: String,
    pub state: String,
    pub in_service: bool,
    pub battery_level: Option<u8>,
    pub charging_state: Option<String>,
    pub battery_range: Option<f64>,
    /// Range added per hour while charging (miles/hour from Fleet API).
    #[serde(default)]
    pub charge_rate: Option<f64>,
    pub charge_limit_soc: Option<u8>,
    #[serde(default)]
    pub charge_limit_soc_min: Option<u8>,
    #[serde(default)]
    pub charge_limit_soc_max: Option<u8>,
    pub locked: Option<bool>,
    /// Front driver window state from vehicle_data (0 = closed, >0 = open).
    #[serde(default)]
    pub fd_window: Option<u8>,
    #[serde(default)]
    pub fp_window: Option<u8>,
    #[serde(default)]
    pub rd_window: Option<u8>,
    #[serde(default)]
    pub rp_window: Option<u8>,
    pub odometer: Option<f64>,
    pub car_version: Option<String>,
    pub inside_temp: Option<f64>,
    pub outside_temp: Option<f64>,
    pub climate_on: Option<bool>,
    pub driver_temp_setting: Option<f64>,
    pub passenger_temp_setting: Option<f64>,
    pub min_avail_temp_celsius: Option<f64>,
    pub max_avail_temp_celsius: Option<f64>,
    pub temperature_units: Option<String>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct VehicleDataResponse {
    pub response: VehicleDataRaw,
}

#[derive(Debug, Deserialize)]
pub struct VehicleDataRaw {
    pub vin: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub in_service: bool,
    #[serde(default)]
    pub charge_state: Option<ChargeStateRaw>,
    #[serde(default)]
    pub vehicle_state: Option<VehicleStateRaw>,
    #[serde(default)]
    pub climate_state: Option<ClimateStateRaw>,
    #[serde(default)]
    pub gui_settings: Option<GuiSettingsRaw>,
}

#[derive(Debug, Deserialize)]
pub struct ChargeStateRaw {
    #[serde(default)]
    pub battery_level: Option<u8>,
    #[serde(default)]
    pub charging_state: Option<String>,
    #[serde(default)]
    pub battery_range: Option<f64>,
    /// Miles of range added per hour (Fleet `charge_state.charge_rate`).
    #[serde(default)]
    pub charge_rate: Option<f64>,
    #[serde(default)]
    pub charge_limit_soc: Option<u8>,
    #[serde(default)]
    pub charge_limit_soc_min: Option<u8>,
    #[serde(default)]
    pub charge_limit_soc_max: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct VehicleStateRaw {
    #[serde(default)]
    pub vehicle_name: Option<String>,
    #[serde(default)]
    pub locked: Option<bool>,
    #[serde(default)]
    pub odometer: Option<f64>,
    #[serde(default)]
    pub car_version: Option<String>,
    /// Window position: 0 closed, non-zero open/vented (Fleet vehicle_data).
    #[serde(default)]
    pub fd_window: Option<u8>,
    #[serde(default)]
    pub fp_window: Option<u8>,
    #[serde(default)]
    pub rd_window: Option<u8>,
    #[serde(default)]
    pub rp_window: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct ClimateStateRaw {
    #[serde(default)]
    pub inside_temp: Option<f64>,
    #[serde(default)]
    pub outside_temp: Option<f64>,
    #[serde(default)]
    pub is_climate_on: Option<bool>,
    #[serde(default)]
    pub driver_temp_setting: Option<f64>,
    #[serde(default)]
    pub passenger_temp_setting: Option<f64>,
    #[serde(default)]
    pub min_avail_temp: Option<f64>,
    #[serde(default)]
    pub max_avail_temp: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct GuiSettingsRaw {
    #[serde(default)]
    pub gui_temperature_units: Option<String>,
}

impl VehicleDetails {
    pub fn from_raw(raw: VehicleDataRaw) -> Self {
        let display_name = resolve_display_name(&raw);

        Self {
            vin: raw.vin,
            display_name,
            state: raw.state,
            in_service: raw.in_service,
            battery_level: raw.charge_state.as_ref().and_then(|c| c.battery_level),
            charging_state: raw
                .charge_state
                .as_ref()
                .and_then(|c| c.charging_state.clone()),
            battery_range: raw
                .charge_state
                .as_ref()
                .and_then(|c| c.battery_range),
            charge_rate: raw.charge_state.as_ref().and_then(|c| c.charge_rate),
            charge_limit_soc: raw
                .charge_state
                .as_ref()
                .and_then(|c| c.charge_limit_soc),
            charge_limit_soc_min: raw
                .charge_state
                .as_ref()
                .and_then(|c| c.charge_limit_soc_min),
            charge_limit_soc_max: raw
                .charge_state
                .as_ref()
                .and_then(|c| c.charge_limit_soc_max),
            locked: raw.vehicle_state.as_ref().and_then(|v| v.locked),
            fd_window: raw.vehicle_state.as_ref().and_then(|v| v.fd_window),
            fp_window: raw.vehicle_state.as_ref().and_then(|v| v.fp_window),
            rd_window: raw.vehicle_state.as_ref().and_then(|v| v.rd_window),
            rp_window: raw.vehicle_state.as_ref().and_then(|v| v.rp_window),
            odometer: raw.vehicle_state.as_ref().and_then(|v| v.odometer),
            car_version: raw
                .vehicle_state
                .as_ref()
                .and_then(|v| v.car_version.clone()),
            inside_temp: convert_temp_for_display(
                raw.climate_state.as_ref().and_then(|c| c.inside_temp),
                raw.gui_settings
                    .as_ref()
                    .and_then(|g| g.gui_temperature_units.as_deref()),
            ),
            outside_temp: convert_temp_for_display(
                raw.climate_state.as_ref().and_then(|c| c.outside_temp),
                raw.gui_settings
                    .as_ref()
                    .and_then(|g| g.gui_temperature_units.as_deref()),
            ),
            climate_on: raw.climate_state.as_ref().and_then(|c| c.is_climate_on),
            driver_temp_setting: convert_temp_for_setting(
                raw.climate_state
                    .as_ref()
                    .and_then(|c| c.driver_temp_setting),
                raw.gui_settings
                    .as_ref()
                    .and_then(|g| g.gui_temperature_units.as_deref()),
            ),
            passenger_temp_setting: convert_temp_for_setting(
                raw.climate_state
                    .as_ref()
                    .and_then(|c| c.passenger_temp_setting),
                raw.gui_settings
                    .as_ref()
                    .and_then(|g| g.gui_temperature_units.as_deref()),
            ),
            min_avail_temp_celsius: raw
                .climate_state
                .as_ref()
                .and_then(|c| c.min_avail_temp),
            max_avail_temp_celsius: raw
                .climate_state
                .as_ref()
                .and_then(|c| c.max_avail_temp),
            temperature_units: raw
                .gui_settings
                .as_ref()
                .and_then(|g| g.gui_temperature_units.clone()),
            fetched_at: Utc::now(),
        }
    }

    pub fn display_temperature_unit(&self) -> &'static str {
        if temperature::uses_fahrenheit(self.temperature_units.as_deref()) {
            "F"
        } else {
            "C"
        }
    }

    pub fn target_temp_setting(&self) -> Option<f64> {
        self.driver_temp_setting
            .or(self.passenger_temp_setting)
    }

    pub fn temp_bounds(&self) -> temperature::CelsiusBounds {
        temperature::resolve_celsius_bounds(
            self.min_avail_temp_celsius,
            self.max_avail_temp_celsius,
        )
    }

    pub fn charge_limit_bounds(&self) -> crate::api::charge_limit::ChargeLimitBounds {
        crate::api::charge_limit::resolve_charge_limit_bounds(
            self.charge_limit_soc_min,
            self.charge_limit_soc_max,
        )
    }

    /// Whether any reported window is open. `None` if no window data is available.
    pub fn any_window_open(&self) -> Option<bool> {
        let states = [self.fd_window, self.fp_window, self.rd_window, self.rp_window];
        if states.iter().all(Option::is_none) {
            return None;
        }
        Some(states.iter().flatten().any(|&state| state > 0))
    }

    /// Human-readable window summary for the details panel.
    pub fn windows_status_label(&self) -> Option<&'static str> {
        match self.any_window_open() {
            Some(true) => Some("open"),
            Some(false) => Some("closed"),
            None => None,
        }
    }

    /// Optimistically mark all reported windows closed (0) or vented (1).
    pub fn set_windows_open_state(&mut self, open: bool) {
        let state = if open { 1 } else { 0 };
        self.fd_window = Some(state);
        self.fp_window = Some(state);
        self.rd_window = Some(state);
        self.rp_window = Some(state);
    }

    pub fn min_temp_display(&self) -> f64 {
        temperature::celsius_to_setting_display(
            self.temp_bounds().min,
            self.temperature_units.as_deref(),
        )
    }

    pub fn max_temp_display(&self) -> f64 {
        temperature::celsius_to_setting_display(
            self.temp_bounds().max,
            self.temperature_units.as_deref(),
        )
    }
}

fn resolve_display_name(raw: &VehicleDataRaw) -> String {
    raw.vehicle_state
        .as_ref()
        .and_then(|state| state.vehicle_name.as_deref())
        .filter(|name| !name.is_empty())
        .or(raw.display_name.as_deref().filter(|name| !name.is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| "Unnamed Vehicle".into())
}

fn convert_temp_for_display(celsius: Option<f64>, gui_units: Option<&str>) -> Option<f64> {
    celsius.map(|value| temperature::celsius_to_display(value, gui_units))
}

fn convert_temp_for_setting(celsius: Option<f64>, gui_units: Option<&str>) -> Option<f64> {
    celsius.map(|value| temperature::celsius_to_setting_display(value, gui_units))
}

#[cfg(test)]
mod tests {
    use super::{temperature, VehicleDataRaw, VehicleDetails};

    #[test]
    fn keeps_celsius_when_gui_uses_c() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "climate_state": { "inside_temp": 21.0, "outside_temp": 10.0 },
            "gui_settings": { "gui_temperature_units": "C" }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert_eq!(details.inside_temp, Some(21.0));
        assert_eq!(details.outside_temp, Some(10.0));
        assert_eq!(details.display_temperature_unit(), "C");
    }

    #[test]
    fn converts_to_fahrenheit_when_gui_uses_f() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "climate_state": { "inside_temp": 22.0, "outside_temp": 0.0 },
            "gui_settings": { "gui_temperature_units": "F" }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert!((details.inside_temp.unwrap() - 71.6).abs() < 0.01);
        assert!((details.outside_temp.unwrap() - 32.0).abs() < 0.01);
        assert_eq!(details.display_temperature_unit(), "F");
    }

    #[test]
    fn parses_driver_and_passenger_temp_settings() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "climate_state": {
                "driver_temp_setting": 22.0,
                "passenger_temp_setting": 23.0
            },
            "gui_settings": { "gui_temperature_units": "C" }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert_eq!(details.driver_temp_setting, Some(22.0));
        assert_eq!(details.passenger_temp_setting, Some(23.0));
        assert_eq!(details.target_temp_setting(), Some(22.0));
    }

    #[test]
    fn rounds_fahrenheit_target_settings() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "climate_state": {
                "driver_temp_setting": 22.0,
                "passenger_temp_setting": 22.0
            },
            "gui_settings": { "gui_temperature_units": "F" }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert_eq!(details.driver_temp_setting, Some(72.0));
        assert_eq!(details.passenger_temp_setting, Some(72.0));
    }

    #[test]
    fn parses_charge_rate() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "charge_state": {
                "charge_rate": 27.5,
                "charging_state": "Charging"
            }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert!((details.charge_rate.unwrap() - 27.5).abs() < f64::EPSILON);
        assert_eq!(details.charging_state.as_deref(), Some("Charging"));
    }

    #[test]
    fn parses_available_temperature_bounds() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "climate_state": {
                "min_avail_temp": 15.0,
                "max_avail_temp": 28.0
            },
            "gui_settings": { "gui_temperature_units": "F" }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert_eq!(details.min_avail_temp_celsius, Some(15.0));
        assert_eq!(details.max_avail_temp_celsius, Some(28.0));
        assert_eq!(details.min_temp_display(), 59.0);
        assert_eq!(details.max_temp_display(), 82.0);
    }

    #[test]
    fn celsius_to_fahrenheit_handles_freezing_and_boiling() {
        assert!((temperature::celsius_to_fahrenheit(0.0) - 32.0).abs() < f64::EPSILON);
        assert!((temperature::celsius_to_fahrenheit(100.0) - 212.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_window_state_and_summarizes() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "vehicle_state": {
                "fd_window": 0,
                "fp_window": 1,
                "rd_window": 0,
                "rp_window": 0
            }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert_eq!(details.fd_window, Some(0));
        assert_eq!(details.fp_window, Some(1));
        assert_eq!(details.any_window_open(), Some(true));
        assert_eq!(details.windows_status_label(), Some("open"));
    }

    #[test]
    fn windows_closed_when_all_zero() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "vehicle_state": {
                "fd_window": 0,
                "fp_window": 0,
                "rd_window": 0,
                "rp_window": 0
            }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);
        assert_eq!(details.any_window_open(), Some(false));
        assert_eq!(details.windows_status_label(), Some("closed"));
    }

    #[test]
    fn uses_vehicle_state_name_for_display_name() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "vehicle_state": { "vehicle_name": "Nikola 2.0" }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert_eq!(details.display_name, "Nikola 2.0");
    }

    #[test]
    fn falls_back_to_unnamed_vehicle_when_name_missing() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111"
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert_eq!(details.display_name, "Unnamed Vehicle");
    }

    #[test]
    fn prefers_vehicle_state_name_over_top_level_display_name() {
        let raw: VehicleDataRaw = serde_json::from_value(serde_json::json!({
            "vin": "5YJSA11111111111",
            "display_name": "Wrong Name",
            "vehicle_state": { "vehicle_name": "Nikola 2.0" }
        }))
        .unwrap();

        let details = VehicleDetails::from_raw(raw);

        assert_eq!(details.display_name, "Nikola 2.0");
    }
}