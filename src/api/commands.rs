#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockAction {
    Lock,
    Unlock,
}

impl LockAction {
    pub fn command_name(self) -> &'static str {
        match self {
            Self::Lock => "door_lock",
            Self::Unlock => "door_unlock",
        }
    }

    pub fn locked(self) -> bool {
        matches!(self, Self::Lock)
    }

    pub fn from_locked(locked: Option<bool>) -> Self {
        if locked == Some(true) {
            Self::Unlock
        } else {
            Self::Lock
        }
    }
}

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

/// Fleet `window_control` command: vent (crack open) or close all windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    Vent,
    Close,
}

impl WindowAction {
    pub fn command_name(self) -> &'static str {
        "window_control"
    }

    pub fn command_body_value(self) -> &'static str {
        match self {
            Self::Vent => "vent",
            Self::Close => "close",
        }
    }

    /// After a successful action, whether any window should be treated as open.
    pub fn windows_open(self) -> bool {
        matches!(self, Self::Vent)
    }

    /// If windows are open, close them; otherwise vent.
    pub fn from_windows_open(windows_open: Option<bool>) -> Self {
        if windows_open == Some(true) {
            Self::Close
        } else {
            Self::Vent
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClimateAction, LockAction, WindowAction};

    #[test]
    fn from_locked_locks_when_unlocked_or_unknown() {
        assert_eq!(LockAction::from_locked(Some(false)), LockAction::Lock);
        assert_eq!(LockAction::from_locked(None), LockAction::Lock);
    }

    #[test]
    fn from_locked_unlocks_when_locked() {
        assert_eq!(LockAction::from_locked(Some(true)), LockAction::Unlock);
    }

    #[test]
    fn from_climate_on_starts_when_off_or_unknown() {
        assert_eq!(ClimateAction::from_climate_on(Some(false)), ClimateAction::Start);
        assert_eq!(ClimateAction::from_climate_on(None), ClimateAction::Start);
    }

    #[test]
    fn from_climate_on_stops_when_on() {
        assert_eq!(ClimateAction::from_climate_on(Some(true)), ClimateAction::Stop);
    }

    #[test]
    fn from_windows_open_vents_when_closed_or_unknown() {
        assert_eq!(WindowAction::from_windows_open(Some(false)), WindowAction::Vent);
        assert_eq!(WindowAction::from_windows_open(None), WindowAction::Vent);
    }

    #[test]
    fn from_windows_open_closes_when_open() {
        assert_eq!(WindowAction::from_windows_open(Some(true)), WindowAction::Close);
    }
}