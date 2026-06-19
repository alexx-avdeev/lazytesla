use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use serde::Deserialize;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;

use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

#[derive(Clone)]
struct AppState {
    result_tx: Arc<Mutex<Option<oneshot::Sender<CallbackResult>>>>,
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

pub struct CallbackServer {
    expected_state: String,
    result_rx: oneshot::Receiver<CallbackResult>,
    shutdown_tx: oneshot::Sender<()>,
    handle: JoinHandle<()>,
}

impl CallbackServer {
    pub async fn start(port: u16, expected_state: String) -> Result<Self> {
        let (result_tx, result_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let state = AppState {
            result_tx: Arc::new(Mutex::new(Some(result_tx))),
        };

        let app = Router::new()
            .route("/callback", get(handle_callback))
            .with_state(state);

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|err| AppError::Callback(format!("failed to bind {addr}: {err}")))?;

        let handle = tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });

            let _ = server.await;
        });

        Ok(Self {
            expected_state,
            result_rx,
            shutdown_tx,
            handle,
        })
    }

    pub async fn wait_for_callback(self, timeout: std::time::Duration) -> Result<CallbackResult> {
        let result = tokio::time::timeout(timeout, self.result_rx)
            .await
            .map_err(|_| AppError::LoginTimeout)?
            .map_err(|_| AppError::Callback("callback channel closed".into()))?;

        if result.state != self.expected_state {
            return Err(AppError::StateMismatch);
        }

        let _ = self.shutdown_tx.send(());
        let _ = self.handle.await;

        Ok(result)
    }
}

async fn handle_callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Html<&'static str> {
    if let Some(tx) = state.result_tx.lock().await.take() {
        let _ = tx.send(CallbackResult {
            code: query.code,
            state: query.state,
        });
    }

    Html(
        "<html><body style=\"font-family: sans-serif; text-align: center; margin-top: 4rem;\">\
         <h1>Authentication successful</h1>\
         <p>You can close this tab and return to LazyTesla.</p>\
         </body></html>",
    )
}