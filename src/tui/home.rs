use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::api::VehicleDetails;
use crate::app::{App, VehiclesStatus};
use crate::util::{format_local_timestamp, mask_vin};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .split(area);

    draw_header(frame, chunks[0], app);
    draw_main(frame, chunks[1], app);
    draw_status(frame, chunks[2], app);
    draw_footer(frame, chunks[3]);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let count = if app.vehicles.is_empty() {
        match &app.vehicles_status {
            VehiclesStatus::Loading => "refreshing...".into(),
            _ => "vehicles".into(),
        }
    } else {
        format!("{} vehicle(s)", app.vehicles.len())
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

fn vehicle_panel_split(width: u16) -> (u16, u16) {
    if width < 100 {
        (25, 75)
    } else {
        (15, 85)
    }
}

fn draw_main(frame: &mut Frame, area: Rect, app: &App) {
    if app.has_cached_vehicles() {
        let (list_pct, details_pct) = vehicle_panel_split(area.width);
        let chunks = Layout::horizontal([
            Constraint::Percentage(list_pct),
            Constraint::Percentage(details_pct),
        ])
        .split(area);
        draw_vehicle_list(frame, chunks[0], app);
        draw_vehicle_details(frame, chunks[1], app);
        return;
    }

    match &app.vehicles_status {
        VehiclesStatus::Idle | VehiclesStatus::Loading => {
            let paragraph = Paragraph::new("Loading vehicles and details...")
                .block(Block::default().borders(Borders::ALL).title("Vehicles"));
            frame.render_widget(paragraph, area);
        }
        VehiclesStatus::Error(message) => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "Failed to refresh",
                    Style::default().fg(Color::Red),
                )),
                Line::from(""),
            ];
            for line in wrap_message(message, 72) {
                lines.push(Line::from(line));
            }
            let paragraph = Paragraph::new(lines)
                .block(Block::default().borders(Borders::ALL).title("Vehicles"));
            frame.render_widget(paragraph, area);
        }
        VehiclesStatus::Loaded => {
            let paragraph = Paragraph::new("No vehicles found on this account.")
                .block(Block::default().borders(Borders::ALL).title("Vehicles"));
            frame.render_widget(paragraph, area);
        }
    }
}

fn draw_vehicle_list(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .vehicles
        .iter()
        .enumerate()
        .map(|(index, vehicle)| {
            let style = if index == app.selected_vehicle {
                Style::default().reversed()
            } else {
                Style::default()
            };
            ListItem::new(vehicle.display_name.as_str()).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Vehicles"),
    );

    frame.render_widget(list, area);
}

fn draw_vehicle_details(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title("Details");

    let Some(vehicle) = app.selected_vehicle() else {
        let paragraph = Paragraph::new("No vehicle selected.").block(block);
        frame.render_widget(paragraph, area);
        return;
    };

    let lines = if let Some(details) = app.selected_vehicle_details() {
        detail_lines(details)
    } else {
        vec![
            Line::from(vec![
                Span::styled(
                    vehicle.display_name.as_str(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(format!("State: {}", vehicle.state)),
            Line::from(format!("VIN: {}", mask_vin(&vehicle.vin))),
            Line::from(""),
            Line::from("No cached details for this vehicle."),
            Line::from("Press r to refresh."),
        ]
    };

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn detail_lines(details: &VehicleDetails) -> Vec<Line<'_>> {
    let temp_unit = details.display_temperature_unit();

    vec![
        Line::from(vec![Span::styled(
            details.display_name.as_str(),
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(format!("State: {}", details.state)),
        Line::from(format!("VIN: {}", mask_vin(&details.vin))),
        Line::from(format!(
            "In service: {}",
            if details.in_service { "yes" } else { "no" }
        )),
        Line::from(""),
        Line::from(Span::styled("Charge", Style::default().fg(Color::Yellow))),
        Line::from(format_option_u8("Battery", details.battery_level, "%")),
        Line::from(format_option_str(
            "Charging",
            details.charging_state.as_deref(),
        )),
        Line::from(format_option_f64(
            "Range",
            details.battery_range,
            "mi",
        )),
        Line::from(format_option_u8(
            "Charge limit",
            details.charge_limit_soc,
            "%",
        )),
        Line::from(""),
        Line::from(Span::styled("Vehicle", Style::default().fg(Color::Yellow))),
        Line::from(format_option_bool("Locked", details.locked)),
        Line::from(format_option_f64("Odometer", details.odometer, "mi")),
        Line::from(format_option_str(
            "Software version",
            details.car_version.as_deref(),
        )),
        Line::from(""),
        Line::from(Span::styled("Climate", Style::default().fg(Color::Yellow))),
        Line::from(format_option_bool("Climate on", details.climate_on)),
        Line::from(format_option_f64(
            "Inside temp",
            details.inside_temp,
            temp_unit,
        )),
        Line::from(format_option_f64(
            "Outside temp",
            details.outside_temp,
            temp_unit,
        )),
        Line::from(""),
        Line::from(format!(
            "Updated at {}",
            format_local_timestamp(details.fetched_at)
        )),
    ]
}

fn format_option_str(label: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{label}: {value}"),
        None => format!("{label}: —"),
    }
}

fn format_option_u8(label: &str, value: Option<u8>, unit: &str) -> String {
    match value {
        Some(value) => format!("{label}: {value}{unit}"),
        None => format!("{label}: —"),
    }
}

fn format_option_f64(label: &str, value: Option<f64>, unit: &str) -> String {
    match value {
        Some(value) => format!("{label}: {value:.1} {unit}"),
        None => format!("{label}: —"),
    }
}

fn format_option_bool(label: &str, value: Option<bool>) -> String {
    match value {
        Some(value) => format!("{label}: {}", if value { "yes" } else { "no" }),
        None => format!("{label}: —"),
    }
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let expires = app
        .expires_at()
        .map(|time| time.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "unknown".into());

    let cache_age = app
        .details_refreshed_at
        .map(|time| time.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "not cached".into());

    let mut text: Vec<Line<'_>> = Vec::new();
    if app.shows_spinner() {
        text.push(Line::from(vec![
            Span::styled(app.refresh_spinner(), Style::default().fg(Color::Cyan)),
            Span::raw(" "),
            Span::raw(app.status_message.as_str()),
        ]));
    } else {
        for line in wrap_message(&app.status_message, 72) {
            text.push(Line::from(line));
        }
    }
    text.push(Line::from(format!("Details cached: {cache_age}")));
    text.push(Line::from(format!("Token expires: {expires}")));

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
        Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" climate   "),
        Span::styled("u", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" lock   "),
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