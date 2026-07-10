use std::collections::HashMap;
use std::path::Path;
use prost::Message;
use rand::RngCore;

use crate::vehicle_command::climate;
use crate::vehicle_command::crypto::key::PrivateKey;
use crate::vehicle_command::crypto::signer::{
    self, build_outbound_message, build_session_info_request, request_id, Signer,
    DEFAULT_EXPIRATION, FLAG_ENCRYPT_RESPONSE,
};
use crate::vehicle_command::error::{Result, VehicleCommandError};
use crate::vehicle_command::fleet::FleetTransport;
use crate::vehicle_command::proto::car_server::{OperationStatusE, Response};
use crate::vehicle_command::proto::universal_message::{Domain, RoutableMessage};
use crate::vehicle_command::session::{process_session_response, SessionStore};

struct VinState {
    routing_address: [u8; 16],
    signers: HashMap<i32, Signer>,
}

pub struct VehicleCommandClient {
    private_key: PrivateKey,
    fleet: FleetTransport,
    sessions: SessionStore,
    vin_states: HashMap<String, VinState>,
}

impl VehicleCommandClient {
    pub fn new(fleet_key_path: &Path, audience: &str) -> Result<Self> {
        Ok(Self {
            private_key: PrivateKey::load(fleet_key_path)?,
            fleet: FleetTransport::new(audience.to_string()),
            sessions: SessionStore::new()?,
            vin_states: HashMap::new(),
        })
    }

    pub async fn climate_on(&mut self, vin: &str, access_token: &str) -> Result<()> {
        self.send_climate(vin, access_token, true).await
    }

    pub async fn climate_off(&mut self, vin: &str, access_token: &str) -> Result<()> {
        self.send_climate(vin, access_token, false).await
    }

    async fn send_climate(&mut self, vin: &str, access_token: &str, power_on: bool) -> Result<()> {
        self.fleet.wake_up(vin, access_token).await?;
        let payload = climate::build_climate_action(power_on)?;
        self.send_infotainment_action(vin, access_token, payload)
            .await
    }

    pub async fn ensure_session(&mut self, vin: &str, access_token: &str, domain: Domain) -> Result<()> {
        if self
            .vin_states
            .get(vin)
            .and_then(|state| state.signers.get(&(domain as i32)))
            .is_some()
        {
            return Ok(());
        }

        if let Some(signer) = self.sessions.try_import_signer(&self.private_key, vin, domain)? {
            self.store_signer(vin, domain, signer);
            return Ok(());
        }

        self.handshake(vin, access_token, domain).await
    }

    pub async fn send_infotainment_action(
        &mut self,
        vin: &str,
        access_token: &str,
        payload: Vec<u8>,
    ) -> Result<()> {
        self.ensure_session(vin, access_token, Domain::Infotainment)
            .await?;

        let routing_address = self.routing_address_for(vin);
        let uuid = random_bytes(16);
        let mut message = build_outbound_message(
            Domain::Infotainment,
            &routing_address,
            &uuid,
            payload,
            FLAG_ENCRYPT_RESPONSE,
        );

        {
            let signer = self
                .signer_mut(vin, Domain::Infotainment)
                .ok_or_else(|| VehicleCommandError::Protocol("session not ready".into()))?;
            signer.authorize_hmac(&mut message, DEFAULT_EXPIRATION)?;
        }

        let req_id = request_id(&message).unwrap_or_default();
        let encoded = encode_message(&message)?;
        let response_bytes = self
            .fleet
            .signed_command(vin, access_token, &encoded)
            .await?;
        let mut response = RoutableMessage::decode(response_bytes.as_slice())?;

        if let Some(err) = signer::routable_message_error(&response) {
            return Err(err);
        }

        {
            let signer = self
                .signer_mut(vin, Domain::Infotainment)
                .ok_or_else(|| VehicleCommandError::Protocol("session not ready".into()))?;
            signer.decrypt(&mut response, &req_id)?;
        }

        if let Some(err) = signer::routable_message_error(&response) {
            return Err(err);
        }

        let response_payload = match &response.payload {
            Some(
                crate::vehicle_command::proto::universal_message::routable_message::Payload::ProtobufMessageAsBytes(
                    bytes,
                ),
            ) => bytes.clone(),
            _ => {
                return Err(VehicleCommandError::Protocol(
                    "missing vehicle response payload".into(),
                ))
            }
        };

        self.parse_car_server_response(&response_payload)
    }

    async fn handshake(&mut self, vin: &str, access_token: &str, domain: Domain) -> Result<()> {
        let routing_address = self.routing_address_for(vin);
        let uuid = random_bytes(16);
        let request = build_session_info_request(
            domain,
            self.private_key.public_bytes(),
            &routing_address,
            &uuid,
        );
        let encoded = encode_message(&request)?;
        let response_bytes = self
            .fleet
            .signed_command(vin, access_token, &encoded)
            .await?;
        let response = RoutableMessage::decode(response_bytes.as_slice())?;
        let signer = process_session_response(&self.private_key, vin, &response)?;
        let exported = signer.export_session_info()?;
        self.sessions
            .update_vin(vin, domain, &exported)?;
        self.store_signer(vin, domain, signer);
        Ok(())
    }

    fn parse_car_server_response(&self, payload: &[u8]) -> Result<()> {
        let response = Response::decode(payload)?;
        let status = response
            .action_status
            .ok_or_else(|| VehicleCommandError::Protocol("missing action status".into()))?;

        if status.result == OperationStatusE::OperationstatusError as i32 {
            let reason = status
                .result_reason
                .and_then(|r| r.reason)
                .and_then(|reason| match reason {
                    crate::vehicle_command::proto::car_server::result_reason::Reason::PlainText(
                        text,
                    ) => Some(text),
                })
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "unspecified error".into());
            return Err(VehicleCommandError::VehicleFault(format!(
                "car could not execute command: {reason}"
            )));
        }

        Ok(())
    }

    fn routing_address_for(&mut self, vin: &str) -> [u8; 16] {
        if let Some(state) = self.vin_states.get(vin) {
            return state.routing_address;
        }
        let mut address = [0_u8; 16];
        rand::thread_rng().fill_bytes(&mut address);
        self.vin_states.insert(
            vin.to_string(),
            VinState {
                routing_address: address,
                signers: HashMap::new(),
            },
        );
        address
    }

    fn store_signer(&mut self, vin: &str, domain: Domain, signer: Signer) {
        let state = self.vin_states.entry(vin.to_string()).or_insert_with(|| {
            let mut address = [0_u8; 16];
            rand::thread_rng().fill_bytes(&mut address);
            VinState {
                routing_address: address,
                signers: HashMap::new(),
            }
        });
        state.signers.insert(domain as i32, signer);
    }

    fn signer_mut(&mut self, vin: &str, domain: Domain) -> Option<&mut Signer> {
        self.vin_states
            .get_mut(vin)
            .and_then(|state| state.signers.get_mut(&(domain as i32)))
    }
}

fn encode_message(message: &RoutableMessage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    message.encode(&mut buf)?;
    Ok(buf)
}

fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0_u8; len];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}