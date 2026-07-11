use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use prost::Message;
use rand::RngCore;

use crate::vehicle_command::climate;
use crate::vehicle_command::crypto::key::PrivateKey;
use crate::vehicle_command::crypto::signer::{
    self, build_outbound_message, build_session_info_request, is_retriable_fault, message_fault_code,
    request_id, Signer, DEFAULT_EXPIRATION, FLAG_ENCRYPT_RESPONSE,
};
use crate::vehicle_command::error::{Result, VehicleCommandError};
use crate::vehicle_command::fleet::FleetTransport;
use crate::vehicle_command::proto::car_server::{OperationStatusE as CarServerOperationStatusE, Response};
use crate::vehicle_command::proto::universal_message::{Domain, RoutableMessage};
use crate::vehicle_command::proto::vcsec::{
    from_vcsec_message::SubMessage as VcsecSubMessage, FromVcsecMessage,
    OperationStatusE as VcsecOperationStatusE,
};
use crate::vehicle_command::security;
use crate::vehicle_command::session::{process_session_response, try_sync_session_from_message, SessionStore};

const MAX_COMMAND_ATTEMPTS: usize = 3;

#[derive(Clone, Copy)]
enum ResponseKind {
    CarServer,
    Vcsec,
}

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

    pub async fn lock(&mut self, vin: &str, access_token: &str) -> Result<()> {
        self.send_lock_action(vin, access_token, true).await
    }

    pub async fn unlock(&mut self, vin: &str, access_token: &str) -> Result<()> {
        self.send_lock_action(vin, access_token, false).await
    }

    async fn send_climate(&mut self, vin: &str, access_token: &str, power_on: bool) -> Result<()> {
        self.fleet.wake_up(vin, access_token).await?;
        let payload = climate::build_climate_action(power_on)?;
        self.send_domain_action(vin, access_token, Domain::Infotainment, payload, ResponseKind::CarServer)
            .await
    }

    async fn send_lock_action(
        &mut self,
        vin: &str,
        access_token: &str,
        lock: bool,
    ) -> Result<()> {
        self.fleet.wake_up(vin, access_token).await?;
        let payload = security::build_rke_action(lock)?;
        self.send_domain_action(
            vin,
            access_token,
            Domain::VehicleSecurity,
            payload,
            ResponseKind::Vcsec,
        )
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
        self.send_domain_action(vin, access_token, Domain::Infotainment, payload, ResponseKind::CarServer)
            .await
    }

    async fn send_domain_action(
        &mut self,
        vin: &str,
        access_token: &str,
        domain: Domain,
        payload: Vec<u8>,
        response_kind: ResponseKind,
    ) -> Result<()> {
        let mut last_err = None;
        for attempt in 0..MAX_COMMAND_ATTEMPTS {
            match self
                .try_send_domain_action(vin, access_token, domain, payload.clone(), response_kind)
                .await
            {
                Ok(()) => return Ok(()),
                Err(err) => {
                    let retriable = matches!(&err, VehicleCommandError::VehicleFault(_))
                        && attempt + 1 < MAX_COMMAND_ATTEMPTS;
                    if retriable {
                        self.clear_signer(vin, domain);
                        if self.handshake(vin, access_token, domain).await.is_err() {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                        last_err = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| {
            VehicleCommandError::Protocol("command failed after retries".into())
        }))
    }

    async fn try_send_domain_action(
        &mut self,
        vin: &str,
        access_token: &str,
        domain: Domain,
        payload: Vec<u8>,
        response_kind: ResponseKind,
    ) -> Result<()> {
        self.ensure_session(vin, access_token, domain).await?;

        let routing_address = self.routing_address_for(vin);
        let uuid = random_bytes(16);
        let mut message = build_outbound_message(
            domain,
            &routing_address,
            &uuid,
            payload,
            FLAG_ENCRYPT_RESPONSE,
        );

        {
            let signer = self
                .signer_mut(vin, domain)
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
            let fault = message_fault_code(&response);
            if is_retriable_fault(fault) {
                if let Some(signer) = self.signer_mut(vin, domain) {
                    if try_sync_session_from_message(signer, &response) {
                        self.persist_signer(vin, domain)?;
                        return Err(err);
                    }
                }
            }
            return Err(err);
        }

        {
            let signer = self
                .signer_mut(vin, domain)
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

        match response_kind {
            ResponseKind::CarServer => self.parse_car_server_response(&response_payload)?,
            ResponseKind::Vcsec => self.parse_vcsec_response(&response_payload)?,
        }
        self.persist_signer(vin, domain)?;
        Ok(())
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
        self.persist_signer_with(vin, domain, &signer)?;
        self.store_signer(vin, domain, signer);
        Ok(())
    }

    fn persist_signer(&mut self, vin: &str, domain: Domain) -> Result<()> {
        let exported = self
            .signer_mut(vin, domain)
            .ok_or_else(|| VehicleCommandError::Protocol("session not ready".into()))?
            .export_session_info()?;
        self.sessions.update_vin(vin, domain, &exported)
    }

    fn persist_signer_with(&mut self, vin: &str, domain: Domain, signer: &Signer) -> Result<()> {
        let exported = signer.export_session_info()?;
        self.sessions.update_vin(vin, domain, &exported)
    }

    fn parse_car_server_response(&self, payload: &[u8]) -> Result<()> {
        let response = Response::decode(payload)?;
        let status = response
            .action_status
            .ok_or_else(|| VehicleCommandError::Protocol("missing action status".into()))?;

        if status.result == CarServerOperationStatusE::OperationstatusError as i32 {
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

    fn parse_vcsec_response(&self, payload: &[u8]) -> Result<()> {
        let response = FromVcsecMessage::decode(payload)?;

        if let Some(VcsecSubMessage::NominalError(err)) = response.sub_message {
            let code = crate::vehicle_command::proto::errors::GenericErrorE::try_from(err.generic_error)
                .map(|value| value.as_str_name().to_string())
                .unwrap_or_else(|_| format!("error code {}", err.generic_error));
            return Err(VehicleCommandError::VehicleFault(format!(
                "vehicle security controller error: {code}"
            )));
        }

        let Some(VcsecSubMessage::CommandStatus(status)) = response.sub_message else {
            return Ok(());
        };

        match status.operation_status {
            value if value == VcsecOperationStatusE::OperationstatusOk as i32 => Ok(()),
            value if value == VcsecOperationStatusE::OperationstatusWait as i32 => {
                Err(VehicleCommandError::VehicleFault(
                    "vehicle security controller is busy".into(),
                ))
            }
            _ => Err(VehicleCommandError::VehicleFault(
                "vehicle security controller rejected command".into(),
            )),
        }
    }

    fn clear_signer(&mut self, vin: &str, domain: Domain) {
        if let Some(state) = self.vin_states.get_mut(vin) {
            state.signers.remove(&(domain as i32));
        }
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