use std::time::Duration;

use lazytesla::api::VehicleRefreshResult;
use lazytesla::app::{
    refresh_vehicles,
    send_climate_command,
    send_lock_command,
    send_set_charge_limit_command,
    send_set_climate_temp_command,
    send_window_command,
    App,
    Screen,
};
use lazytesla::auth::oauth::{OAuthClient, TokenSet};
use lazytesla::auth::server::CallbackServer;
use lazytesla::config::Config;
use lazytesla::error::{AppError, Result};
use lazytesla::tui;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    // Load project .env when present; existing shell variables take precedence.
    let _ = dotenvy::dotenv();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            eprintln!();
            eprintln!("Set TESLA_CLIENT_ID and TESLA_CLIENT_SECRET before running.");
            std::process::exit(1);
        }
    };

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, config).await;
    ratatui::restore();
    result
}

async fn run(terminal: &mut ratatui::DefaultTerminal, config: Config) -> Result<()> {
    let mut app = App::new(config).await?;
    let (auth_tx, mut auth_rx) = mpsc::unbounded_channel::<Result<TokenSet>>();
    let (refresh_tx, mut refresh_rx) =
        mpsc::unbounded_channel::<Result<VehicleRefreshResult>>();
    let (climate_tx, mut climate_rx) =
        mpsc::unbounded_channel::<lazytesla::app::ClimateCommandOutcome>();
    let (lock_tx, mut lock_rx) =
        mpsc::unbounded_channel::<lazytesla::app::LockCommandOutcome>();
    let (temp_tx, mut temp_rx) =
        mpsc::unbounded_channel::<lazytesla::app::SetClimateTempCommandOutcome>();
    let (charge_limit_tx, mut charge_limit_rx) =
        mpsc::unbounded_channel::<lazytesla::app::SetChargeLimitCommandOutcome>();
    let (window_tx, mut window_rx) =
        mpsc::unbounded_channel::<lazytesla::app::WindowCommandOutcome>();

    if app.is_authenticated() {
        request_vehicle_refresh(&mut app, refresh_tx.clone());
    }

    loop {
        app.tick_spinner();
        terminal.draw(|frame| tui::draw(frame, &app))?;

        if let Ok(result) = auth_rx.try_recv() {
            match result {
                Ok(tokens) => {
                    if let Err(err) = app.set_authenticated(tokens).await {
                        app.set_error(err);
                    } else {
                        request_vehicle_refresh(&mut app, refresh_tx.clone());
                    }
                }
                Err(err) => app.set_error(err),
            }
        }

        if let Ok(result) = refresh_rx.try_recv() {
            app.apply_vehicle_refresh(result);
        }

        if let Ok(outcome) = climate_rx.try_recv() {
            app.apply_climate_command(&outcome.vin, outcome.result);
        }

        if let Ok(outcome) = lock_rx.try_recv() {
            app.apply_lock_command(&outcome.vin, outcome.result);
        }

        if let Ok(outcome) = temp_rx.try_recv() {
            app.apply_set_climate_temp(&outcome.vin, outcome.result);
        }

        if let Ok(outcome) = charge_limit_rx.try_recv() {
            app.apply_set_charge_limit(&outcome.vin, outcome.result);
        }

        if let Ok(outcome) = window_rx.try_recv() {
            app.apply_window_command(&outcome.vin, outcome.result);
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if app.screen == Screen::Home && app.is_editing_temp() {
                    if handle_temp_input_key(&mut app, key.code, temp_tx.clone()) {
                        continue;
                    }
                }

                if app.screen == Screen::Home && app.is_editing_charge_limit() {
                    if handle_charge_limit_input_key(&mut app, key.code, charge_limit_tx.clone()) {
                        continue;
                    }
                }

                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Enter if app.screen == Screen::Auth => {
                        if let Err(err) = start_login(&mut app, auth_tx.clone()) {
                            app.set_error(err);
                        }
                    }
                    KeyCode::Char('l') if app.screen == Screen::Home => {
                        if let Err(err) = app.logout() {
                            app.set_error(err);
                        }
                    }
                    KeyCode::Char('r') if app.screen == Screen::Home => {
                        request_vehicle_refresh(&mut app, refresh_tx.clone());
                    }
                    KeyCode::Up | KeyCode::Char('k') if app.screen == Screen::Home => {
                        app.select_previous_vehicle();
                    }
                    KeyCode::Down | KeyCode::Char('j') if app.screen == Screen::Home => {
                        app.select_next_vehicle();
                    }
                    KeyCode::Char('c') if app.screen == Screen::Home => {
                        request_climate_toggle(&mut app, climate_tx.clone());
                    }
                    KeyCode::Char('u') if app.screen == Screen::Home => {
                        request_lock_toggle(&mut app, lock_tx.clone());
                    }
                    KeyCode::Char('w') if app.screen == Screen::Home => {
                        request_window_toggle(&mut app, window_tx.clone());
                    }
                    KeyCode::Char('t') if app.screen == Screen::Home => {
                        app.begin_temp_input();
                    }
                    KeyCode::Char('b') if app.screen == Screen::Home => {
                        app.begin_charge_limit_input();
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn handle_temp_input_key(
    app: &mut App,
    code: KeyCode,
    temp_tx: mpsc::UnboundedSender<lazytesla::app::SetClimateTempCommandOutcome>,
) -> bool {
    match code {
        KeyCode::Enter => {
            if let Some(request) = app.submit_temp_input() {
                tokio::spawn(async move {
                    let outcome = send_set_climate_temp_command(request).await;
                    let _ = temp_tx.send(outcome);
                });
            }
            true
        }
        KeyCode::Esc => {
            app.cancel_temp_input();
            true
        }
        KeyCode::Backspace => {
            app.backspace_temp_input();
            true
        }
        KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Up => {
            let delta = app
                .selected_vehicle_details()
                .map(|details| lazytesla::api::temp_adjust_step(details.temperature_units.as_deref()))
                .unwrap_or(0.5);
            app.adjust_temp_input(delta);
            true
        }
        KeyCode::Char('-') | KeyCode::Char('_') | KeyCode::Down => {
            let delta = app
                .selected_vehicle_details()
                .map(|details| lazytesla::api::temp_adjust_step(details.temperature_units.as_deref()))
                .unwrap_or(0.5);
            app.adjust_temp_input(-delta);
            true
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() || ch == '.' => {
            app.append_temp_input(ch);
            true
        }
        // Let quit through; swallow other home shortcuts while the modal has focus.
        KeyCode::Char('q') => false,
        _ => true,
    }
}

fn handle_charge_limit_input_key(
    app: &mut App,
    code: KeyCode,
    charge_limit_tx: mpsc::UnboundedSender<lazytesla::app::SetChargeLimitCommandOutcome>,
) -> bool {
    match code {
        KeyCode::Enter => {
            if let Some(request) = app.submit_charge_limit_input() {
                tokio::spawn(async move {
                    let outcome = send_set_charge_limit_command(request).await;
                    let _ = charge_limit_tx.send(outcome);
                });
            }
            true
        }
        KeyCode::Esc => {
            app.cancel_charge_limit_input();
            true
        }
        KeyCode::Backspace => {
            app.backspace_charge_limit_input();
            true
        }
        KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Up => {
            app.adjust_charge_limit_input(i16::from(lazytesla::api::CHARGE_LIMIT_STEP));
            true
        }
        KeyCode::Char('-') | KeyCode::Char('_') | KeyCode::Down => {
            app.adjust_charge_limit_input(-i16::from(lazytesla::api::CHARGE_LIMIT_STEP));
            true
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() => {
            app.append_charge_limit_input(ch);
            true
        }
        KeyCode::Char('q') => false,
        _ => true,
    }
}

fn request_climate_toggle(
    app: &mut App,
    climate_tx: mpsc::UnboundedSender<lazytesla::app::ClimateCommandOutcome>,
) {
    let Some(request) = app.climate_toggle_request() else {
        return;
    };

    app.begin_climate_command(request.action);

    tokio::spawn(async move {
        let outcome = send_climate_command(request).await;
        let _ = climate_tx.send(outcome);
    });
}

fn request_lock_toggle(
    app: &mut App,
    lock_tx: mpsc::UnboundedSender<lazytesla::app::LockCommandOutcome>,
) {
    let Some(request) = app.lock_toggle_request() else {
        return;
    };

    app.begin_lock_command(request.action);

    tokio::spawn(async move {
        let outcome = send_lock_command(request).await;
        let _ = lock_tx.send(outcome);
    });
}

fn request_window_toggle(
    app: &mut App,
    window_tx: mpsc::UnboundedSender<lazytesla::app::WindowCommandOutcome>,
) {
    let Some(request) = app.window_toggle_request() else {
        return;
    };

    app.begin_window_command(request.action);

    tokio::spawn(async move {
        let outcome = send_window_command(request).await;
        let _ = window_tx.send(outcome);
    });
}

fn request_vehicle_refresh(
    app: &mut App,
    refresh_tx: mpsc::UnboundedSender<Result<VehicleRefreshResult>>,
) {
    let Some(request) = app.vehicle_load_request() else {
        return;
    };

    app.begin_vehicle_refresh();

    tokio::spawn(async move {
        let result = refresh_vehicles(request).await;
        let _ = refresh_tx.send(result);
    });
}

fn start_login(app: &mut App, auth_tx: mpsc::UnboundedSender<Result<TokenSet>>) -> Result<()> {
    let (url, port, state) = app.start_login()?;
    let oauth = OAuthClient::new(app.config().clone());
    let timeout = app.login_timeout();

    tokio::spawn(async move {
        let result = perform_login(oauth, url, port, state, timeout).await;
        let _ = auth_tx.send(result);
    });

    Ok(())
}

async fn perform_login(
    oauth: OAuthClient,
    url: String,
    port: u16,
    state: String,
    timeout: Duration,
) -> Result<TokenSet> {
    let server = CallbackServer::start(port, state).await?;

    open::that(&url).map_err(|err| AppError::Auth(format!("failed to open browser: {err}")))?;

    let callback = server.wait_for_callback(timeout).await?;
    oauth.exchange_code(&callback.code).await
}