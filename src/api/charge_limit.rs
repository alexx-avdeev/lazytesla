/// Default Tesla charge-limit range (percent SOC).
pub const MIN_CHARGE_LIMIT_PERCENT: u8 = 50;
pub const MAX_CHARGE_LIMIT_PERCENT: u8 = 100;
pub const CHARGE_LIMIT_STEP: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChargeLimitBounds {
    pub min: u8,
    pub max: u8,
}

impl Default for ChargeLimitBounds {
    fn default() -> Self {
        Self {
            min: MIN_CHARGE_LIMIT_PERCENT,
            max: MAX_CHARGE_LIMIT_PERCENT,
        }
    }
}

pub fn resolve_charge_limit_bounds(
    min_avail: Option<u8>,
    max_avail: Option<u8>,
) -> ChargeLimitBounds {
    ChargeLimitBounds {
        min: min_avail.unwrap_or(MIN_CHARGE_LIMIT_PERCENT),
        max: max_avail.unwrap_or(MAX_CHARGE_LIMIT_PERCENT),
    }
}

pub fn clamp_charge_limit(value: i16, bounds: ChargeLimitBounds) -> u8 {
    value.clamp(i16::from(bounds.min), i16::from(bounds.max)) as u8
}

pub fn parse_charge_limit(input: &str, bounds: ChargeLimitBounds) -> Result<u8, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("enter a charge limit".into());
    }

    let value: u8 = trimmed
        .parse()
        .map_err(|_| format!("invalid charge limit: {trimmed}"))?;

    if !(bounds.min..=bounds.max).contains(&value) {
        return Err(format!(
            "charge limit must be between {}% and {}%",
            bounds.min, bounds.max
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_in_range() {
        let bounds = ChargeLimitBounds::default();
        assert_eq!(parse_charge_limit("80", bounds).unwrap(), 80);
    }

    #[test]
    fn parse_rejects_below_min() {
        let bounds = ChargeLimitBounds::default();
        assert!(parse_charge_limit("40", bounds).is_err());
    }

    #[test]
    fn clamp_respects_bounds() {
        let bounds = ChargeLimitBounds::default();
        assert_eq!(clamp_charge_limit(120, bounds), 100);
        assert_eq!(clamp_charge_limit(30, bounds), 50);
    }
}
