use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, AuthStatus};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(5),
        Constraint::Length(3),
    ])
    .split(area);

    draw_header(frame, chunks[0]);
    draw_instructions(frame, chunks[1]);
    draw_status(frame, chunks[2], app);
    draw_footer(frame, chunks[3]);
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "LazyTesla",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  —  Tesla Fleet API"),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Authentication"),
    );
    frame.render_widget(title, area);
}

fn draw_instructions(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from("Sign in with your Tesla developer application credentials."),
        Line::from(""),
        Line::from("1. Press Enter to open the Tesla login page in your browser."),
        Line::from("2. Sign in and grant the requested permissions."),
        Line::from("3. Your browser will redirect to localhost and return here automatically."),
        Line::from(""),
        Line::from("Required environment variables:"),
        Line::from("  TESLA_CLIENT_ID"),
        Line::from("  TESLA_CLIENT_SECRET"),
        Line::from("  TESLA_DOMAIN (your developer app domain)"),
        Line::from(""),
        Line::from("Optional:"),
        Line::from("  TESLA_REDIRECT_URI (default: http://localhost:8484/callback)"),
        Line::from("  TESLA_AUDIENCE (default: NA fleet API)"),
        Line::from("  TESLA_CALLBACK_PORT (default: 8484)"),
    ];

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("How it works"))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let status_style = match &app.auth_status {
        AuthStatus::NotAuthenticated => Style::default().fg(Color::Yellow),
        AuthStatus::WaitingForBrowser => Style::default().fg(Color::Blue),
        AuthStatus::ExchangingToken => Style::default().fg(Color::Blue),
        AuthStatus::Authenticated => Style::default().fg(Color::Green),
        AuthStatus::Error(_) => Style::default().fg(Color::Red),
    };

    let status_label = match &app.auth_status {
        AuthStatus::NotAuthenticated => "Not authenticated",
        AuthStatus::WaitingForBrowser => "Waiting for browser login",
        AuthStatus::ExchangingToken => "Exchanging token",
        AuthStatus::Authenticated => "Authenticated",
        AuthStatus::Error(_) => "Error",
    };

    let text = vec![
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(status_label, status_style.add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(app.status_message.as_str()),
    ];

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Status"))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" sign in   "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" quit"),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Keys"));

    frame.render_widget(help, area);
}