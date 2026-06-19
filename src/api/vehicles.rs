use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Vehicle {
    pub id: String,
    pub vin: String,
    pub display_name: String,
    pub state: String,
    pub in_service: bool,
}

#[derive(Debug, Deserialize)]
pub struct VehiclesResponse {
    pub response: Vec<VehicleRaw>,
    #[serde(default)]
    pub count: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct VehicleRaw {
    #[serde(default)]
    pub id_s: String,
    pub vin: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub in_service: bool,
}

impl From<VehicleRaw> for Vehicle {
    fn from(raw: VehicleRaw) -> Self {
        let display_name = raw
            .display_name
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Unnamed Vehicle".into());

        Self {
            id: raw.id_s,
            vin: raw.vin,
            display_name,
            state: raw.state,
            in_service: raw.in_service,
        }
    }
}