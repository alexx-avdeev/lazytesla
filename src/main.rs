use std::time::Duration;

use lazytesla::api::VehicleRefreshResult;
use lazytesla::app::{refresh_vehicles, App, Screen};
use lazytesla::auth::oauth::{OAuthClient, TokenSet};
use lazytesla::auth::server::CallbackServer;
use lazytesla::config::Config;
use lazytesla::error::{AppError, Result};
use lazytesla::tui;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
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

    if app.is_authenticated() {
        request_vehicle_refresh(&mut app, refresh_tx.clone());
    }

    loop {
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

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
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
                    _ => {}
                }
            }
        }
    }

    Ok(())
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