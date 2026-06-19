use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::app::{App, VehiclesStatus};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .split(area);

    draw_header(frame, chunks[0], app);
    draw_vehicles(frame, chunks[1], app);
    draw_status(frame, chunks[2], app);
    draw_footer(frame, chunks[3]);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let count = match &app.vehicles_status {
        VehiclesStatus::Loaded => format!("{} vehicle(s)", app.vehicles.len()),
        VehiclesStatus::Loading => "loading...".into(),
        _ => "vehicles".into(),
    };

    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "LazyTesla",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  —  {count}")),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Home"));

    frame.render_widget(title, area);
}

fn draw_vehicles(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title("Your Vehicles");

    match &app.vehicles_status {
        VehiclesStatus::Idle | VehiclesStatus::Loading => {
            let message = if app.vehicles_status == VehiclesStatus::Loading {
                "Loading vehicles..."
            } else {
                "Waiting to load vehicles..."
            };
            let paragraph = Paragraph::new(message).block(block);
            frame.render_widget(paragraph, area);
        }
        VehiclesStatus::Error(message) => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "Failed to load vehicles",
                    Style::default().fg(Color::Red),
                )),
                Line::from(""),
            ];

            for line in wrap_message(message, 72) {
                lines.push(Line::from(line));
            }

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }
        VehiclesStatus::Loaded if app.vehicles.is_empty() => {
            let paragraph =
                Paragraph::new("No vehicles found on this account.").block(block);
            frame.render_widget(paragraph, area);
        }
        VehiclesStatus::Loaded => {
            let header = Row::new(vec!["Name", "VIN", "State", "Service"])
                .style(Style::default().add_modifier(Modifier::BOLD))
                .bottom_margin(1);

            let rows: Vec<Row> = app
                .vehicles
                .iter()
                .enumerate()
                .map(|(index, vehicle)| {
                    let state_style = state_style(&vehicle.state);
                    let service = if vehicle.in_service {
                        Span::styled("yes", Style::default().fg(Color::Yellow))
                    } else {
                        Span::raw("no")
                    };

                    let row = Row::new(vec![
                        Cell::from(vehicle.display_name.as_str()),
                        Cell::from(mask_vin(&vehicle.vin)),
                        Cell::from(Span::styled(vehicle.state.as_str(), state_style)),
                        Cell::from(Line::from(service)),
                    ]);

                    if index == app.selected_vehicle {
                        row.style(Style::default().reversed())
                    } else {
                        row
                    }
                })
                .collect();

            let table = Table::new(
                rows,
                [
                    Constraint::Percentage(30),
                    Constraint::Percentage(35),
                    Constraint::Percentage(20),
                    Constraint::Percentage(15),
                ],
            )
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().reversed());

            frame.render_widget(table, area);
        }
    }
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let expires = app
        .expires_at()
        .map(|time| time.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "unknown".into());

    let text = vec![
        Line::from(app.status_message.as_str()),
        Line::from(format!("Token expires: {expires}")),
    ];

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Status"))
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled("↑/k", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" up   "),
        Span::styled("↓/j", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" down   "),
        Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" refresh   "),
        Span::styled("l", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" logout   "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" quit"),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Keys"));

    frame.render_widget(help, area);
}

fn wrap_message(message: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in message.split('\n') {
        let mut start = 0;
        while start < paragraph.len() {
            let end = (start + width).min(paragraph.len());
            let mut slice_end = end;
            if end < paragraph.len() {
                if let Some(space) = paragraph[start..end].rfind(' ') {
                    slice_end = start + space;
                }
            }
            if slice_end == start {
                slice_end = end;
            }
            lines.push(paragraph[start..slice_end].trim().to_string());
            start = slice_end;
            while start < paragraph.len() && paragraph.as_bytes()[start] == b' ' {
                start += 1;
            }
        }
        if paragraph.is_empty() {
            lines.push(String::new());
        }
    }
    lines
}

fn mask_vin(vin: &str) -> String {
    let chars: Vec<char> = vin.chars().collect();
    match chars.len() {
        0 => String::new(),
        1 | 2 => vin.to_string(),
        len => {
            let mut masked = String::with_capacity(len);
            masked.push(chars[0]);
            masked.extend(std::iter::repeat_n('*', len - 2));
            masked.push(chars[len - 1]);
            masked
        }
    }
}

fn state_style(state: &str) -> Style {
    match state {
        "online" => Style::default().fg(Color::Green),
        "asleep" => Style::default().fg(Color::Blue),
        "offline" => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::Yellow),
    }
}