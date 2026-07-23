use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::api::{format_temp, VehicleDetails};
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
    draw_footer(frame, chunks[3], app);

    if app.is_editing_temp() {
        draw_temp_modal(frame, area, app);
    } else if app.is_editing_charge_limit() {
        draw_charge_limit_modal(frame, area, app);
    } else if app.is_help_open() {
        draw_help_modal(frame, area, app);
    }
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
        Line::from(format_option_f64(
            "Charge rate",
            details.charge_rate,
            "mi/h",
        )),
        Line::from(format_option_u8(
            "Charge limit",
            details.charge_limit_soc,
            "%",
        )),
        Line::from(""),
        Line::from(Span::styled("Vehicle", Style::default().fg(Color::Yellow))),
        Line::from(format_option_bool("Locked", details.locked)),
        Line::from(format_option_str(
            "Windows",
            details.windows_status_label(),
        )),
        Line::from(format_option_f64("Odometer", details.odometer, "mi")),
        Line::from(format_option_str(
            "Software version",
            details.car_version.as_deref(),
        )),
        Line::from(""),
        Line::from(Span::styled("Climate", Style::default().fg(Color::Yellow))),
        Line::from(format_option_bool("Climate on", details.climate_on)),
        Line::from(format_option_f64(
            "Target temp",
            details.target_temp_setting(),
            temp_unit,
        )),
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
    // While a modal is open, validation errors render there — keep status quiet.
    if !app.is_modal_open() {
        if app.shows_spinner() {
            text.push(Line::from(vec![
                Span::styled(app.refresh_spinner(), Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::raw(app.status_message.as_str()),
            ]));
        } else if !app.status_message.is_empty() {
            for line in wrap_message(&app.status_message, 72) {
                text.push(Line::from(line));
            }
        }
    }
    text.push(Line::from(format!("Details cached: {cache_age}")));
    text.push(Line::from(format!("Token expires: {expires}")));

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Status"))
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(paragraph, area);
}

fn draw_temp_modal(frame: &mut Frame, area: Rect, app: &App) {
    let Some(buffer) = app.temp_input.as_deref() else {
        return;
    };

    let details = app.selected_vehicle_details();
    let unit = details
        .map(|d| d.display_temperature_unit())
        .unwrap_or("C");
    let range = details
        .map(|d| {
            format!(
                "{}–{}°{unit}",
                format_temp(d.min_temp_display()),
                format_temp(d.max_temp_display()),
            )
        })
        .unwrap_or_else(|| "—".into());

    let error = modal_error_message(app.status_message.as_str(), "temperature");

    let height = if error.is_some() { 9 } else { 7 };
    let width = 48u16.min(area.width.saturating_sub(2).max(20));
    let modal_area = centered_rect(width, height, area);

    frame.render_widget(Clear, modal_area);

    let border_style = Style::default().fg(Color::Cyan);
    let hints = modal_step_hints();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            " Set target temperature ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(hints);

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let value_line = Line::from(Span::styled(
        format!("{buffer}°{unit}"),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));

    let mut lines = vec![
        Line::from(""),
        value_line,
        Line::from(""),
        Line::from(vec![
            Span::styled("Range: ", Style::default().fg(Color::DarkGray)),
            Span::raw(range),
        ]),
    ];

    if let Some(err) = error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            err,
            Style::default().fg(Color::Red),
        )));
    }

    let paragraph = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(paragraph, inner);
}

fn draw_charge_limit_modal(frame: &mut Frame, area: Rect, app: &App) {
    let Some(buffer) = app.charge_limit_input.as_deref() else {
        return;
    };

    let details = app.selected_vehicle_details();
    let range = details
        .map(|d| {
            let bounds = d.charge_limit_bounds();
            format!("{}–{}%", bounds.min, bounds.max)
        })
        .unwrap_or_else(|| "50–100%".into());

    let error = modal_error_message(app.status_message.as_str(), "charge limit");

    let height = if error.is_some() { 9 } else { 7 };
    let width = 48u16.min(area.width.saturating_sub(2).max(20));
    let modal_area = centered_rect(width, height, area);

    frame.render_widget(Clear, modal_area);

    let border_style = Style::default().fg(Color::Cyan);
    let hints = modal_step_hints();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            " Set charge limit ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(hints);

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let value_line = Line::from(Span::styled(
        format!("{buffer}%"),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));

    let mut lines = vec![
        Line::from(""),
        value_line,
        Line::from(""),
        Line::from(vec![
            Span::styled("Range: ", Style::default().fg(Color::DarkGray)),
            Span::raw(range),
        ]),
    ];

    if let Some(err) = error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            err,
            Style::default().fg(Color::Red),
        )));
    }

    let paragraph = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(paragraph, inner);
}

fn modal_step_hints() -> Line<'static> {
    Line::from(vec![
        Span::raw("[ "),
        Span::styled("+/-/↑/↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" step  "),
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" send  "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" cancel ]"),
    ])
    .centered()
}

fn modal_error_message<'a>(status: &'a str, keyword: &str) -> Option<&'a str> {
    let msg = status.trim();
    if msg.is_empty() {
        None
    } else if msg.contains(keyword) || msg.contains("invalid") || msg.starts_with("enter ") {
        Some(msg)
    } else {
        None
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

fn draw_help_modal(frame: &mut Frame, area: Rect, app: &App) {
    let entries = app.help_entries();
    let selected = app.help_selected_index().unwrap_or(0);

    let height = (entries.len() as u16)
        .saturating_add(4)
        .min(area.height.saturating_sub(2))
        .max(8);
    let width = 56u16.min(area.width.saturating_sub(2).max(30));
    let modal_area = centered_rect(width, height, area);

    frame.render_widget(Clear, modal_area);

    let hints = Line::from(vec![
        Span::raw("[ "),
        Span::styled("↑/↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" move  "),
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" run  "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" close ]"),
    ])
    .centered();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Hotkeys ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(hints);

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let style = if index == selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let line = format!(" {:<10}  {}", entry.keys, entry.description);
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let help = if app.is_help_open() {
        Paragraph::new(Line::from(vec![
            Span::styled("↑/↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" move   "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" run   "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" close   "),
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" quit"),
        ]))
    } else if app.is_editing_temp() || app.is_editing_charge_limit() {
        Paragraph::new(Line::from(vec![
            Span::styled("digits", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" type   "),
            Span::styled("+/-/↑/↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" step   "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" send   "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" cancel"),
        ]))
    } else {
        Paragraph::new(Line::from(vec![
            Span::styled("↑/k", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" up   "),
            Span::styled("↓/j", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" down   "),
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" refresh   "),
            Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" climate   "),
            Span::styled("t", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" temp   "),
            Span::styled("b", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" limit   "),
            Span::styled("u", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" lock   "),
            Span::styled("w", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" windows   "),
            Span::styled("?", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" help   "),
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" quit"),
        ]))
    }
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