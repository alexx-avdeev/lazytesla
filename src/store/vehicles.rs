use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::api::{Vehicle, VehicleDetails};
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredVehicleCache {
    pub vehicles: Vec<Vehicle>,
    pub details: HashMap<String, VehicleDetails>,
    pub selected_vehicle: usize,
    pub details_refreshed_at: Option<DateTime<Utc>>,
    pub saved_at: DateTime<Utc>,
}

pub struct VehicleStore {
    path: PathBuf,
}

impl VehicleStore {
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "lazytesla")
            .ok_or_else(|| AppError::Store("could not determine config directory".into()))?;

        fs::create_dir_all(dirs.config_dir())?;

        Ok(Self {
            path: dirs.config_dir().join("vehicles.json"),
        })
    }

    pub fn load(&self) -> Result<Option<StoredVehicleCache>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&self.path)?;
        let cache = serde_json::from_str(&contents)
            .map_err(|err| AppError::Store(format!("invalid vehicle cache file: {err}")))?;
        Ok(Some(cache))
    }

    pub fn save(&self, cache: &StoredVehicleCache) -> Result<()> {
        let contents = serde_json::to_string_pretty(cache)
            .map_err(|err| AppError::Store(format!("failed to serialize vehicle cache: {err}")))?;

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

    #[cfg(test)]
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{StoredVehicleCache, VehicleStore};

    #[test]
    fn saves_and_loads_vehicle_cache() {
        let dir = std::env::temp_dir().join(format!("lazytesla-vehicles-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("vehicles.json");
        let store = VehicleStore::with_path(path);
        let cache = StoredVehicleCache {
            vehicles: vec![crate::api::Vehicle {
                id: "1".into(),
                vin: "5YJSA11111111111".into(),
                display_name: "Car 1".into(),
                state: "online".into(),
                in_service: false,
            }],
            details: HashMap::new(),
            selected_vehicle: 0,
            details_refreshed_at: None,
            saved_at: chrono::Utc::now(),
        };

        store.save(&cache).unwrap();
        let loaded = store.load().unwrap().expect("cache should exist");

        assert_eq!(loaded.vehicles.len(), 1);
        assert_eq!(loaded.vehicles[0].display_name, "Car 1");

        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}