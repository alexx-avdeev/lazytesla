use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::auth::oauth::TokenSet;
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
}

impl From<TokenSet> for StoredTokens {
    fn from(tokens: TokenSet) -> Self {
        Self {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: tokens.expires_at,
        }
    }
}

pub struct TokenStore {
    path: PathBuf,
}

impl TokenStore {
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "lazytesla")
            .ok_or_else(|| AppError::Store("could not determine config directory".into()))?;

        fs::create_dir_all(dirs.config_dir())?;

        Ok(Self {
            path: dirs.config_dir().join("tokens.json"),
        })
    }

    pub fn load(&self) -> Result<Option<StoredTokens>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&self.path)?;
        let tokens = serde_json::from_str(&contents)
            .map_err(|err| AppError::Store(format!("invalid token file: {err}")))?;
        Ok(Some(tokens))
    }

    pub fn save(&self, tokens: &StoredTokens) -> Result<()> {
        let contents = serde_json::to_string_pretty(tokens)
            .map_err(|err| AppError::Store(format!("failed to serialize tokens: {err}")))?;

        fs::write(&self.path, contents)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    pub fn is_expired(tokens: &StoredTokens) -> bool {
        tokens.expires_at <= Utc::now()
    }
}