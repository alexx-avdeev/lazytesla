pub const MIN_TEMP_CELSIUS: f64 = 15.0;
pub const MAX_TEMP_CELSIUS: f64 = 32.0;

pub fn uses_fahrenheit(gui_units: Option<&str>) -> bool {
    gui_units
        .map(str::trim)
        .is_some_and(|units| units.eq_ignore_ascii_case("F"))
}

pub fn celsius_to_fahrenheit(celsius: f64) -> f64 {
    celsius * 9.0 / 5.0 + 32.0
}

pub fn fahrenheit_to_celsius(fahrenheit: f64) -> f64 {
    (fahrenheit - 32.0) * 5.0 / 9.0
}

pub fn celsius_to_display(celsius: f64, gui_units: Option<&str>) -> f64 {
    if uses_fahrenheit(gui_units) {
        celsius_to_fahrenheit(celsius)
    } else {
        celsius
    }
}

/// Converts a Celsius HVAC target to display units. Fahrenheit values are rounded
/// to the nearest degree (e.g. 22°C → 72°F).
pub fn celsius_to_setting_display(celsius: f64, gui_units: Option<&str>) -> f64 {
    if uses_fahrenheit(gui_units) {
        celsius_to_fahrenheit(celsius).round()
    } else {
        celsius
    }
}

pub fn display_to_celsius(value: f64, gui_units: Option<&str>) -> f64 {
    if uses_fahrenheit(gui_units) {
        fahrenheit_to_celsius(value)
    } else {
        value
    }
}

pub fn format_temp(value: f64) -> String {
    if (value - value.round()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// Rounds to the nearest 0.5°C step expected by the vehicle HVAC API.
pub fn round_celsius_for_api(celsius: f64) -> f64 {
    (celsius * 2.0).round() / 2.0
}

pub fn temp_adjust_step(gui_units: Option<&str>) -> f64 {
    if uses_fahrenheit(gui_units) {
        1.0
    } else {
        0.5
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CelsiusBounds {
    pub min: f64,
    pub max: f64,
}

pub fn resolve_celsius_bounds(
    min_avail_celsius: Option<f64>,
    max_avail_celsius: Option<f64>,
) -> CelsiusBounds {
    CelsiusBounds {
        min: min_avail_celsius.unwrap_or(MIN_TEMP_CELSIUS),
        max: max_avail_celsius.unwrap_or(MAX_TEMP_CELSIUS),
    }
}

pub fn validate_celsius(celsius: f64, bounds: CelsiusBounds) -> Result<f64, String> {
    if !(bounds.min..=bounds.max).contains(&celsius) {
        return Err(bounds_error_message(bounds, None));
    }
    Ok(celsius)
}

fn bounds_error_message(bounds: CelsiusBounds, gui_units: Option<&str>) -> String {
    let unit = if uses_fahrenheit(gui_units) { "F" } else { "C" };
    let min_display = celsius_to_setting_display(bounds.min, gui_units);
    let max_display = celsius_to_setting_display(bounds.max, gui_units);
    format!(
        "temperature must be between {}°{unit} and {}°{unit}",
        format_temp(min_display),
        format_temp(max_display),
    )
}

pub fn clamp_setting_display(
    value: f64,
    bounds: CelsiusBounds,
    gui_units: Option<&str>,
) -> f64 {
    let min = celsius_to_setting_display(bounds.min, gui_units);
    let max = celsius_to_setting_display(bounds.max, gui_units);
    value.clamp(min, max)
}

pub fn parse_display_temperature(
    input: &str,
    gui_units: Option<&str>,
    bounds: CelsiusBounds,
) -> Result<f64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("enter a temperature".into());
    }

    let display: f64 = trimmed
        .parse()
        .map_err(|_| format!("invalid temperature: {trimmed}"))?;
    let celsius = display_to_celsius(display, gui_units);
    if !(bounds.min..=bounds.max).contains(&celsius) {
        return Err(bounds_error_message(bounds, gui_units));
    }
    Ok(celsius)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_to_celsius_converts_fahrenheit() {
        assert!((display_to_celsius(72.0, Some("F")) - 22.222).abs() < 0.01);
    }

    #[test]
    fn celsius_to_display_converts_to_fahrenheit() {
        assert!((celsius_to_display(22.0, Some("F")) - 71.6).abs() < 0.01);
    }

    #[test]
    fn celsius_to_setting_display_rounds_fahrenheit() {
        assert!((celsius_to_setting_display(22.0, Some("F")) - 72.0).abs() < f64::EPSILON);
        assert!((celsius_to_setting_display(21.1, Some("F")) - 70.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_display_temperature_accepts_celsius() {
        let bounds = CelsiusBounds {
            min: 15.0,
            max: 28.0,
        };
        let celsius = parse_display_temperature("22", Some("C"), bounds).expect("parse");
        assert!((celsius - 22.0).abs() < f64::EPSILON);
    }

    #[test]
    fn validate_rejects_out_of_range() {
        let bounds = CelsiusBounds {
            min: 15.0,
            max: 28.0,
        };
        assert!(validate_celsius(10.0, bounds).is_err());
    }

    #[test]
    fn round_celsius_for_api_snaps_to_half_degree() {
        assert!((round_celsius_for_api(22.222) - 22.0).abs() < f64::EPSILON);
        assert!((round_celsius_for_api(22.3) - 22.5).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_rejects_below_vehicle_min_in_display_units() {
        let bounds = CelsiusBounds {
            min: 15.0,
            max: 28.0,
        };
        assert!(parse_display_temperature("58", Some("F"), bounds).is_err());
    }

    #[test]
    fn clamp_setting_display_respects_bounds_in_fahrenheit() {
        let bounds = CelsiusBounds {
            min: 15.0,
            max: 28.0,
        };
        assert_eq!(clamp_setting_display(90.0, bounds, Some("F")), 82.0);
        assert_eq!(clamp_setting_display(50.0, bounds, Some("F")), 59.0);
    }
}