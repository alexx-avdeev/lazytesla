use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct VehicleDetails {
    pub vin: String,
    pub display_name: String,
    pub state: String,
    pub in_service: bool,
    pub battery_level: Option<u8>,
    pub charging_state: Option<String>,
    pub battery_range: Option<f64>,
    pub charge_limit_soc: Option<u8>,
    pub locked: Option<bool>,
    pub odometer: Option<f64>,
    pub car_version: Option<String>,
    pub inside_temp: Option<f64>,
    pub outside_temp: Option<f64>,
    pub climate_on: Option<bool>,
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
    #[serde(default)]
    pub charge_limit_soc: Option<u8>,
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
}

#[derive(Debug, Deserialize)]
pub struct ClimateStateRaw {
    #[serde(default)]
    pub inside_temp: Option<f64>,
    #[serde(default)]
    pub outside_temp: Option<f64>,
    #[serde(default)]
    pub is_climate_on: Option<bool>,
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
            charge_limit_soc: raw
                .charge_state
                .as_ref()
                .and_then(|c| c.charge_limit_soc),
            locked: raw.vehicle_state.as_ref().and_then(|v| v.locked),
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
            temperature_units: raw
                .gui_settings
                .as_ref()
                .and_then(|g| g.gui_temperature_units.clone()),
            fetched_at: Utc::now(),
        }
    }

    pub fn display_temperature_unit(&self) -> &'static str {
        if uses_fahrenheit(self.temperature_units.as_deref()) {
            "F"
        } else {
            "C"
        }
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
    celsius.map(|value| {
        if uses_fahrenheit(gui_units) {
            celsius_to_fahrenheit(value)
        } else {
            value
        }
    })
}

fn uses_fahrenheit(gui_units: Option<&str>) -> bool {
    gui_units
        .map(str::trim)
        .is_some_and(|units| units.eq_ignore_ascii_case("F"))
}

fn celsius_to_fahrenheit(celsius: f64) -> f64 {
    celsius * 9.0 / 5.0 + 32.0
}

#[cfg(test)]
mod tests {
    use super::{celsius_to_fahrenheit, VehicleDataRaw, VehicleDetails};

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
    fn celsius_to_fahrenheit_handles_freezing_and_boiling() {
        assert!((celsius_to_fahrenheit(0.0) - 32.0).abs() < f64::EPSILON);
        assert!((celsius_to_fahrenheit(100.0) - 212.0).abs() < f64::EPSILON);
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