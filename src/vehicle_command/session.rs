use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::vehicle_command::crypto::key::PrivateKey;
use crate::vehicle_command::crypto::signer::Signer;
use crate::vehicle_command::error::{Result, VehicleCommandError};
use crate::vehicle_command::proto::signatures::signature_data::SigType;
use crate::vehicle_command::proto::universal_message::routable_message::{Payload, SubSigData};
use crate::vehicle_command::proto::universal_message::{Domain, RoutableMessage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub created_at: DateTime<Utc>,
    pub domain: i32,
    #[serde(with = "base64_serde")]
    pub data: Vec<u8>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SessionCache {
    #[serde(rename = "MaxEntries", default)]
    pub max_entries: i32,
    #[serde(rename = "Vehicles", default)]
    pub vehicles: HashMap<String, Vec<CacheEntry>>,
}

pub struct SessionStore {
    path: PathBuf,
    cache: SessionCache,
}

impl SessionStore {
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "lazytesla")
            .ok_or_else(|| VehicleCommandError::Protocol("could not determine config directory".into()))?;
        fs::create_dir_all(dirs.config_dir())?;
        let path = dirs.config_dir().join("session_cache.json");
        let cache = if path.exists() {
            let contents = fs::read_to_string(&path)?;
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            SessionCache::default()
        };
        Ok(Self { path, cache })
    }

    pub fn try_import_signer(
        &self,
        private: &PrivateKey,
        vin: &str,
        domain: Domain,
    ) -> Result<Option<Signer>> {
        let Some(entries) = self.cache.vehicles.get(vin) else {
            return Ok(None);
        };
        let Some(entry) = entries.iter().find(|e| e.domain == domain as i32) else {
            return Ok(None);
        };
        let generated_at: SystemTime = entry.created_at.into();
        let signer = Signer::import_session_info(private, vin.as_bytes(), &entry.data, generated_at)?;
        Ok(Some(signer))
    }

    pub fn update_vin(&mut self, vin: &str, domain: Domain, session_info: &[u8]) -> Result<()> {
        let entry = CacheEntry {
            created_at: Utc::now(),
            domain: domain as i32,
            data: session_info.to_vec(),
        };
        self.cache
            .vehicles
            .entry(vin.to_string())
            .or_default()
            .retain(|e| e.domain != domain as i32);
        self.cache
            .vehicles
            .get_mut(vin)
            .expect("vin entry")
            .push(entry);
        self.save()
    }

    pub fn save(&self) -> Result<()> {
        let contents = serde_json::to_string_pretty(&self.cache)?;
        fs::write(&self.path, contents)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn process_session_response(
    private: &PrivateKey,
    vin: &str,
    response: &RoutableMessage,
) -> Result<Signer> {
    if let Some(err) = super::crypto::signer::routable_message_error(response) {
        return Err(err);
    }

    let challenge = &response.request_uuid;
    let session_info_bytes = match &response.payload {
        Some(Payload::SessionInfo(bytes)) => bytes.clone(),
        _ => {
            return Err(VehicleCommandError::Protocol(
                "missing session info in handshake response".into(),
            ))
        }
    };

    let tag = response
        .sub_sig_data
        .as_ref()
        .and_then(|sub| match sub {
            SubSigData::SignatureData(sig) => sig.sig_type.as_ref(),
        })
        .and_then(|sig_type| match sig_type {
            SigType::SessionInfoTag(data) => Some(data.tag.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            VehicleCommandError::Protocol("missing session info authentication tag".into())
        })?;

    Signer::new_authenticated(private, vin.as_bytes(), challenge, &session_info_bytes, &tag)
}

/// Sync session state from a command error response (vehicle includes updated SessionInfo).
pub fn try_sync_session_from_message(signer: &mut Signer, response: &RoutableMessage) -> bool {
    let challenge = response.request_uuid.clone();
    let session_info_bytes = match &response.payload {
        Some(Payload::SessionInfo(bytes)) => bytes.clone(),
        _ => return false,
    };
    let tag = response
        .sub_sig_data
        .as_ref()
        .and_then(|sub| match sub {
            SubSigData::SignatureData(sig) => sig.sig_type.as_ref(),
        })
        .and_then(|sig_type| match sig_type {
            SigType::SessionInfoTag(data) => Some(data.tag.clone()),
            _ => None,
        });
    let Some(tag) = tag else {
        return false;
    };
    signer
        .update_signed_session_info(&challenge, &session_info_bytes, &tag)
        .is_ok()
}

mod base64_serde {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        STANDARD
            .decode(encoded)
            .map_err(serde::de::Error::custom)
    }
}