/// Runnable home-screen commands (hotkeys and help palette).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeCommand {
    PreviousVehicle,
    NextVehicle,
    Refresh,
    ClimateToggle,
    SetTemp,
    SetChargeLimit,
    LockToggle,
    WindowToggle,
    /// Start or stop charging (Alt+C).
    ChargeToggle,
    Logout,
    Quit,
}

#[derive(Debug, Clone, Copy)]
pub struct HelpEntry {
    pub keys: &'static str,
    pub description: &'static str,
    pub command: HomeCommand,
}

/// Full hotkey list shown in the `?` help modal.
pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry {
        keys: "↑ / k",
        description: "Previous vehicle",
        command: HomeCommand::PreviousVehicle,
    },
    HelpEntry {
        keys: "↓ / j",
        description: "Next vehicle",
        command: HomeCommand::NextVehicle,
    },
    HelpEntry {
        keys: "r",
        description: "Refresh vehicles and details",
        command: HomeCommand::Refresh,
    },
    HelpEntry {
        keys: "c",
        description: "Toggle climate on/off",
        command: HomeCommand::ClimateToggle,
    },
    HelpEntry {
        keys: "t",
        description: "Set target temperature",
        command: HomeCommand::SetTemp,
    },
    HelpEntry {
        keys: "b",
        description: "Set charge limit",
        command: HomeCommand::SetChargeLimit,
    },
    HelpEntry {
        keys: "Alt+c",
        description: "Start or stop charging",
        command: HomeCommand::ChargeToggle,
    },
    HelpEntry {
        keys: "u",
        description: "Toggle lock/unlock",
        command: HomeCommand::LockToggle,
    },
    HelpEntry {
        keys: "w",
        description: "Vent or close windows",
        command: HomeCommand::WindowToggle,
    },
    HelpEntry {
        keys: "l",
        description: "Log out",
        command: HomeCommand::Logout,
    },
    HelpEntry {
        keys: "q",
        description: "Quit",
        command: HomeCommand::Quit,
    },
];
