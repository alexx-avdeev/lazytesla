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

#[cfg(test)]
mod tests {
    use super::{Vehicle, VehicleRaw, VehiclesResponse};

    #[test]
    fn deserializes_vehicle_list_response() {
        let body = r#"{
            "response": [{
                "id_s": "12345678901234567",
                "vin": "5YJSA11111111111",
                "display_name": "Nikola 2.0",
                "state": "online",
                "in_service": false
            }],
            "count": 1
        }"#;

        let parsed: VehiclesResponse = serde_json::from_str(body).unwrap();
        let vehicle = Vehicle::from(parsed.response.into_iter().next().unwrap());

        assert_eq!(vehicle.id, "12345678901234567");
        assert_eq!(vehicle.vin, "5YJSA11111111111");
        assert_eq!(vehicle.display_name, "Nikola 2.0");
        assert_eq!(vehicle.state, "online");
        assert!(!vehicle.in_service);
    }

    #[test]
    fn uses_default_display_name_when_missing() {
        let vehicle = Vehicle::from(VehicleRaw {
            id_s: "1".into(),
            vin: "5YJSA11111111111".into(),
            display_name: None,
            state: "asleep".into(),
            in_service: true,
        });

        assert_eq!(vehicle.display_name, "Unnamed Vehicle");
        assert!(vehicle.in_service);
    }
}