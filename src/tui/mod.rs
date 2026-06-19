pub mod auth;
pub mod home;

use ratatui::Frame;

use crate::app::{App, Screen};

pub fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Auth => auth::draw(frame, app),
        Screen::Home => home::draw(frame, app),
    }
}