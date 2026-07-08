#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClimateAction {
    Start,
    Stop,
}

impl ClimateAction {
    pub fn command_name(self) -> &'static str {
        match self {
            Self::Start => "auto_conditioning_start",
            Self::Stop => "auto_conditioning_stop",
        }
    }

    pub fn climate_on(self) -> bool {
        matches!(self, Self::Start)
    }

    pub fn from_climate_on(climate_on: Option<bool>) -> Self {
        if climate_on == Some(true) {
            Self::Stop
        } else {
            Self::Start
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ClimateAction;

    #[test]
    fn from_climate_on_starts_when_off_or_unknown() {
        assert_eq!(ClimateAction::from_climate_on(Some(false)), ClimateAction::Start);
        assert_eq!(ClimateAction::from_climate_on(None), ClimateAction::Start);
    }

    #[test]
    fn from_climate_on_stops_when_on() {
        assert_eq!(ClimateAction::from_climate_on(Some(true)), ClimateAction::Stop);
    }
}