use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use directories::ProjectDirs;

use crate::error::{AppError, Result};

const ENV_FLAG: &str = "TESLA_DEBUG_CURL";
const ENV_LOG_PATH: &str = "TESLA_DEBUG_CURL_LOG";
const DEFAULT_LOG_FILE: &str = "fleet-api.log";

pub fn is_enabled() -> bool {
    matches!(
        std::env::var(ENV_FLAG).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

pub fn log_get(url: &str, headers: &[(&str, &str)]) {
    if !is_enabled() {
        return;
    }
    let _ = write_log(&format_curl_get(url, headers));
}

pub fn log_post_json(url: &str, headers: &[(&str, &str)], body: &str) {
    if !is_enabled() {
        return;
    }
    let _ = write_log(&format_curl_post_json(url, headers, body));
}

fn write_log(command: &str) -> Result<()> {
    let path = resolve_log_path()?;
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let entry = format!("\n=== {timestamp} ===\n{command}\n");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| {
            AppError::Store(format!(
                "failed to open fleet debug log {}: {err}",
                path.display()
            ))
        })?;

    file.write_all(entry.as_bytes()).map_err(|err| {
        AppError::Store(format!(
            "failed to write fleet debug log {}: {err}",
            path.display()
        ))
    })?;

    Ok(())
}

fn resolve_log_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var(ENV_LOG_PATH) {
        if !path.is_empty() {
            let path = PathBuf::from(path);
            ensure_parent_dir(&path)?;
            return Ok(path);
        }
    }

    let dirs = ProjectDirs::from("", "", "lazytesla")
        .ok_or_else(|| AppError::Store("could not determine config directory".into()))?;

    fs::create_dir_all(dirs.config_dir()).map_err(|err| {
        AppError::Store(format!(
            "failed to create config directory {}: {err}",
            dirs.config_dir().display()
        ))
    })?;

    Ok(dirs.config_dir().join(DEFAULT_LOG_FILE))
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    fs::create_dir_all(parent).map_err(|err| {
        AppError::Store(format!(
            "failed to create directory {}: {err}",
            parent.display()
        ))
    })
}

fn format_curl_get(url: &str, headers: &[(&str, &str)]) -> String {
    let mut command = format!("curl -sS -X GET {}", shell_quote(url));
    for (name, value) in headers {
        command.push_str(&format!(" \\\n  -H {}", shell_quote(&format!("{name}: {value}"))));
    }
    command
}

fn format_curl_post_json(url: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut command = format!("curl -sS -X POST {}", shell_quote(url));
    for (name, value) in headers {
        command.push_str(&format!(" \\\n  -H {}", shell_quote(&format!("{name}: {value}"))));
    }
    command.push_str(&format!(" \\\n  -d {}", shell_quote(body)));
    command
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{Mutex, MutexGuard};

    use super::{format_curl_get, format_curl_post_json, log_get, log_post_json, shell_quote};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner())
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn log_get_formats_curl_command() {
        let url = "https://fleet.example/api/1/vehicles";
        let headers = [
            ("Authorization", "Bearer user-token"),
            ("Content-Type", "application/json"),
        ];
        let command = format_curl_get(url, &headers);
        assert!(command.contains("curl -sS -X GET 'https://fleet.example/api/1/vehicles'"));
        assert!(command.contains("-H 'Authorization: Bearer user-token'"));
        assert!(command.contains("-H 'Content-Type: application/json'"));
        let _ = log_get;
    }

    #[test]
    fn log_post_json_formats_curl_command() {
        let url = "https://fleet.example/api/1/partner_accounts";
        let headers = [
            ("Authorization", "Bearer partner-token"),
            ("Content-Type", "application/json"),
        ];
        let body = r#"{"domain":"example.com"}"#;
        let command = format_curl_post_json(url, &headers, body);
        assert!(command.contains("curl -sS -X POST"));
        assert!(command.contains(r#"-d '{"domain":"example.com"}'"#));
        let _ = log_post_json;
    }

    #[test]
    fn writes_curl_command_to_log_file() {
        let _lock = env_lock();
        let log_dir = std::env::temp_dir().join(format!(
            "lazytesla-fleet-debug-{}",
            std::process::id()
        ));
        let log_path = log_dir.join("fleet-api.log");
        let _ = fs::remove_dir_all(&log_dir);

        // SAFETY: test holds ENV_LOCK so no other test reads these vars concurrently.
        unsafe {
            std::env::set_var("TESLA_DEBUG_CURL", "1");
            std::env::set_var("TESLA_DEBUG_CURL_LOG", log_path.to_string_lossy().as_ref());
        }

        log_get(
            "https://fleet.example/api/1/vehicles",
            &[
                ("Authorization", "Bearer user-token"),
                ("Content-Type", "application/json"),
            ],
        );

        assert!(log_dir.exists());
        let contents = fs::read_to_string(&log_path).expect("log file should exist");
        assert!(contents.contains("=== "));
        assert!(contents.contains("curl -sS -X GET 'https://fleet.example/api/1/vehicles'"));

        let _ = fs::remove_dir_all(&log_dir);
        unsafe {
            std::env::remove_var("TESLA_DEBUG_CURL");
            std::env::remove_var("TESLA_DEBUG_CURL_LOG");
        }
    }
}