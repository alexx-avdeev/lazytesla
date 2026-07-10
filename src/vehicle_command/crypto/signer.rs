use std::time::{Duration, SystemTime};

use prost::Message;

use crate::vehicle_command::crypto::ecdh::Session;
use crate::vehicle_command::crypto::key::PrivateKey;
use crate::vehicle_command::crypto::metadata::Metadata;
use crate::vehicle_command::error::{Result, VehicleCommandError};
use crate::vehicle_command::proto::signatures::{
    signature_data::SigType, HmacPersonalizedSignatureData, KeyIdentity, SessionInfo,
    SignatureData, SignatureType, Tag,
};
use crate::vehicle_command::proto::universal_message::{
    destination::SubDestination, routable_message::{Payload, SubSigData},
    Destination, Domain, Flags, MessageFaultE, OperationStatusE, RoutableMessage,
};

pub const EPOCH_ID_LENGTH: usize = 16;
pub const ADDRESS_LENGTH: usize = 16;
pub const UUID_LENGTH: usize = 16;
pub const DEFAULT_EXPIRATION: Duration = Duration::from_secs(5);
pub const FLAG_ENCRYPT_RESPONSE: u32 = 1 << Flags::FlagEncryptResponse as u32;

pub struct Signer {
    verifier_name: Vec<u8>,
    session: Session,
    counter: u32,
    epoch: [u8; EPOCH_ID_LENGTH],
    time_zero: SystemTime,
    set_time: u32,
    verifier_public_bytes: Vec<u8>,
}

impl Signer {
    pub fn from_session_info(
        private: &PrivateKey,
        verifier_name: &[u8],
        info: &SessionInfo,
    ) -> Result<Self> {
        if verifier_name.len() > 255 {
            return Err(VehicleCommandError::Crypto("metadata field too long".into()));
        }
        let session = Session::exchange(private, &info.public_key)?;
        let mut epoch = [0_u8; EPOCH_ID_LENGTH];
        let epoch_copy = info.epoch.len().min(EPOCH_ID_LENGTH);
        epoch[..epoch_copy].copy_from_slice(&info.epoch[..epoch_copy]);

        Ok(Self {
            verifier_name: verifier_name.to_vec(),
            session,
            counter: info.counter,
            epoch,
            time_zero: epoch_start_time(info.clock_time),
            set_time: info.clock_time,
            verifier_public_bytes: info.public_key.clone(),
        })
    }

    pub fn import_session_info(
        private: &PrivateKey,
        verifier_name: &[u8],
        encoded_info: &[u8],
        generated_at: SystemTime,
    ) -> Result<Self> {
        let info = SessionInfo::decode(encoded_info)?;
        let mut signer = Self::from_session_info(private, verifier_name, &info)?;
        let clock = Duration::from_secs(info.clock_time as u64);
        signer.time_zero = generated_at.checked_sub(clock).unwrap_or(generated_at);
        Ok(signer)
    }

    pub fn new_authenticated(
        private: &PrivateKey,
        verifier_name: &[u8],
        challenge: &[u8],
        encoded_info: &[u8],
        tag: &[u8],
    ) -> Result<Self> {
        let signer =
            Self::import_session_info(private, verifier_name, encoded_info, SystemTime::now())?;
        let valid_tag = signer
            .session
            .session_info_hmac(verifier_name, challenge, encoded_info)?;
        if !constant_time_eq(&valid_tag, tag) {
            return Err(VehicleCommandError::Crypto("session info hmac invalid".into()));
        }
        Ok(signer)
    }

    pub fn export_session_info(&self) -> Result<Vec<u8>> {
        let info = SessionInfo {
            counter: self.counter,
            public_key: self.verifier_public_bytes.clone(),
            epoch: self.epoch.to_vec(),
            clock_time: self.timestamp(),
            status: 0,
            handle: 0,
        };
        let mut buf = Vec::new();
        info.encode(&mut buf)?;
        Ok(buf)
    }

    pub fn update_signed_session_info(
        &mut self,
        challenge: &[u8],
        encoded_info: &[u8],
        tag: &[u8],
    ) -> Result<()> {
        let valid_tag = self
            .session
            .session_info_hmac(&self.verifier_name, challenge, encoded_info)?;
        if !constant_time_eq(&valid_tag, tag) {
            return Err(VehicleCommandError::Crypto("session info hmac invalid".into()));
        }
        let info = SessionInfo::decode(encoded_info)?;
        self.update_session_info(&info)
    }

    fn update_session_info(&mut self, info: &SessionInfo) -> Result<()> {
        if info.public_key != self.verifier_public_bytes {
            return Err(VehicleCommandError::Crypto(
                "session info public key mismatch".into(),
            ));
        }
        if self.epoch != epoch_bytes(info) || self.set_time <= info.clock_time {
            if self.counter < info.counter {
                self.counter = info.counter;
            }
            self.epoch = epoch_bytes(info);
            self.set_time = info.clock_time;
            self.time_zero = epoch_start_time(info.clock_time);
        }
        Ok(())
    }

    pub fn authorize_hmac(&mut self, message: &mut RoutableMessage, expires_in: Duration) -> Result<()> {
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or_else(|| VehicleCommandError::Crypto("counter rollover".into()))?;

        let expires_at = self.expires_at(expires_in);
        let mut hmac_data = HmacPersonalizedSignatureData {
            epoch: self.epoch.to_vec(),
            counter: self.counter,
            expires_at,
            tag: Vec::new(),
        };
        hmac_data.tag = self.hmac_tag(message, &hmac_data)?;

        message.sub_sig_data = Some(SubSigData::SignatureData(SignatureData {
            signer_identity: Some(KeyIdentity {
                identity_type: Some(
                    crate::vehicle_command::proto::signatures::key_identity::IdentityType::PublicKey(
                        self.session.local_public_bytes().to_vec(),
                    ),
                ),
            }),
            sig_type: Some(SigType::HmacPersonalizedData(hmac_data)),
        }));

        Ok(())
    }

    pub fn decrypt(&self, message: &mut RoutableMessage, request_id: &[u8]) -> Result<u32> {
        let signature_data = message
            .sub_sig_data
            .as_ref()
            .and_then(|s| match s {
                SubSigData::SignatureData(data) => Some(data),
            })
            .ok_or_else(|| VehicleCommandError::Protocol("missing signature data".into()))?;

        let (counter, nonce, tag) = match &signature_data.sig_type {
            Some(SigType::AesGcmResponseData(data)) => {
                (data.counter, data.nonce.clone(), data.tag.clone())
            }
            _ => {
                return Err(VehicleCommandError::Protocol(
                    "missing AES-GCM response data".into(),
                ))
            }
        };

        let authenticated_data = self.response_metadata(message, request_id, counter)?;
        let ciphertext = payload_bytes(message);

        let plaintext = self.session.decrypt(&nonce, &ciphertext, &authenticated_data, &tag)?;

        message.payload = Some(Payload::ProtobufMessageAsBytes(plaintext));
        message.sub_sig_data = None;
        Ok(counter)
    }

    fn hmac_tag(
        &self,
        message: &RoutableMessage,
        hmac_data: &HmacPersonalizedSignatureData,
    ) -> Result<Vec<u8>> {
        let mut meta = Metadata::with_hmac(self.session.new_hmac("authenticated command"));
        self.extract_metadata(
            &mut meta,
            message,
            hmac_data.counter,
            hmac_data.expires_at,
            SignatureType::HmacPersonalized,
        )?;
        Ok(meta.checksum(&payload_bytes(message)))
    }

    fn extract_metadata(
        &self,
        meta: &mut Metadata,
        message: &RoutableMessage,
        counter: u32,
        expires_at: u32,
        method: SignatureType,
    ) -> Result<()> {
        meta.add(Tag::SignatureType, &[method as u8])?;

        let domain = message
            .to_destination
            .as_ref()
            .and_then(|dest| match &dest.sub_destination {
                Some(SubDestination::Domain(domain)) => Some(*domain as u8),
                _ => None,
            })
            .ok_or_else(|| VehicleCommandError::Protocol("domain missing".into()))?;

        meta.add(Tag::Domain, &[domain])?;
        meta.add(Tag::Personalization, &self.verifier_name)?;
        meta.add(Tag::Epoch, &self.epoch)?;
        meta.add_u32(Tag::ExpiresAt, expires_at)?;
        meta.add_u32(Tag::Counter, counter)?;

        if message.flags > 0 {
            meta.add_u32(Tag::Flags, message.flags)?;
        }

        Ok(())
    }

    fn response_metadata(
        &self,
        message: &RoutableMessage,
        request_id: &[u8],
        counter: u32,
    ) -> Result<Vec<u8>> {
        let mut meta = Metadata::new_sha256();
        meta.add(Tag::SignatureType, &[SignatureType::AesGcmResponse as u8])?;
        let domain = message
            .from_destination
            .as_ref()
            .and_then(|dest| match &dest.sub_destination {
                Some(SubDestination::Domain(domain)) => Some(*domain as u8),
                _ => None,
            })
            .unwrap_or(0);
        meta.add(Tag::Domain, &[domain])?;
        meta.add(Tag::Personalization, &self.verifier_name)?;
        meta.add_u32(Tag::Counter, counter)?;
        meta.add_u32(Tag::Flags, message.flags)?;
        meta.add(Tag::RequestHash, request_id)?;
        let fault = message
            .signed_message_status
            .as_ref()
            .map(|s| s.signed_message_fault)
            .unwrap_or(MessageFaultE::MessagefaultErrorNone as i32);
        meta.add_u32(Tag::Fault, fault as u32)?;
        Ok(meta.checksum(&[]))
    }

    fn expires_at(&self, expires_in: Duration) -> u32 {
        let now = SystemTime::now();
        let elapsed = now.duration_since(self.time_zero).unwrap_or_default();
        (elapsed + expires_in).as_secs() as u32
    }

    fn timestamp(&self) -> u32 {
        SystemTime::now()
            .duration_since(self.time_zero)
            .unwrap_or_default()
            .as_secs() as u32
    }
}

pub fn payload_bytes(message: &RoutableMessage) -> Vec<u8> {
    match &message.payload {
        Some(Payload::ProtobufMessageAsBytes(bytes)) => bytes.clone(),
        _ => Vec::new(),
    }
}

pub fn request_id(message: &RoutableMessage) -> Option<Vec<u8>> {
    let signature_data = match &message.sub_sig_data {
        Some(SubSigData::SignatureData(data)) => data,
        _ => return None,
    };

    match &signature_data.sig_type {
        Some(SigType::HmacPersonalizedData(data)) => {
            let mut id = vec![SignatureType::HmacPersonalized as u8];
            id.extend_from_slice(&data.tag);
            Some(id)
        }
        Some(SigType::AesGcmPersonalizedData(data)) => {
            let mut id = vec![SignatureType::AesGcmPersonalized as u8];
            id.extend_from_slice(&data.tag);
            Some(id)
        }
        _ => None,
    }
}

pub fn message_fault_code(message: &RoutableMessage) -> i32 {
    message
        .signed_message_status
        .as_ref()
        .map(|s| s.signed_message_fault)
        .unwrap_or(MessageFaultE::MessagefaultErrorNone as i32)
}

pub fn is_retriable_fault(fault: i32) -> bool {
    matches!(
        fault,
        x if x == MessageFaultE::MessagefaultErrorBusy as i32
            || x == MessageFaultE::MessagefaultErrorTimeout as i32
            || x == MessageFaultE::MessagefaultErrorInvalidSignature as i32
            || x == MessageFaultE::MessagefaultErrorInvalidTokenOrCounter as i32
            || x == MessageFaultE::MessagefaultErrorInternal as i32
            || x == MessageFaultE::MessagefaultErrorIncorrectEpoch as i32
            || x == MessageFaultE::MessagefaultErrorTimeExpired as i32
            || x == MessageFaultE::MessagefaultErrorTimeToLiveTooLong as i32
    )
}

fn fault_description(fault: i32) -> String {
    match fault {
        x if x == MessageFaultE::MessagefaultErrorInvalidTokenOrCounter as i32 => {
            "anti-replay counter out of sync (stale session cache)".into()
        }
        x if x == MessageFaultE::MessagefaultErrorInvalidSignature as i32 => {
            "invalid command signature".into()
        }
        x if x == MessageFaultE::MessagefaultErrorIncorrectEpoch as i32 => {
            "session epoch mismatch".into()
        }
        x if x == MessageFaultE::MessagefaultErrorTimeExpired as i32 => {
            "command expired".into()
        }
        x if x == MessageFaultE::MessagefaultErrorBusy as i32 => "vehicle busy".into(),
        x if x == MessageFaultE::MessagefaultErrorTimeout as i32 => "vehicle timeout".into(),
        other => format!("fault code {other}"),
    }
}

pub fn routable_message_error(message: &RoutableMessage) -> Option<VehicleCommandError> {
    let fault = message_fault_code(message);

    if fault != MessageFaultE::MessagefaultErrorNone as i32 {
        if fault == MessageFaultE::MessagefaultErrorUnknownKeyId as i32 {
            return Some(VehicleCommandError::KeyNotPaired);
        }
        return Some(VehicleCommandError::VehicleFault(fault_description(fault)));
    }

    if let Some(status) = &message.signed_message_status {
        if status.operation_status == OperationStatusE::OperationstatusWait as i32 {
            return Some(VehicleCommandError::VehicleFault("vehicle busy".into()));
        }
    }

    None
}

pub fn build_outbound_message(
    domain: Domain,
    routing_address: &[u8],
    uuid: &[u8],
    payload: Vec<u8>,
    flags: u32,
) -> RoutableMessage {
    RoutableMessage {
        to_destination: Some(Destination {
            sub_destination: Some(SubDestination::Domain(domain as i32)),
        }),
        from_destination: Some(Destination {
            sub_destination: Some(SubDestination::RoutingAddress(
                routing_address.to_vec(),
            )),
        }),
        payload: Some(Payload::ProtobufMessageAsBytes(payload)),
        sub_sig_data: None,
        signed_message_status: None,
        request_uuid: Vec::new(),
        uuid: uuid.to_vec(),
        flags,
    }
}

pub fn build_session_info_request(
    domain: Domain,
    public_bytes: &[u8],
    routing_address: &[u8],
    uuid: &[u8],
) -> RoutableMessage {
    RoutableMessage {
        to_destination: Some(Destination {
            sub_destination: Some(SubDestination::Domain(domain as i32)),
        }),
        from_destination: Some(Destination {
            sub_destination: Some(SubDestination::RoutingAddress(
                routing_address.to_vec(),
            )),
        }),
        payload: Some(Payload::SessionInfoRequest(
            crate::vehicle_command::proto::universal_message::SessionInfoRequest {
                public_key: public_bytes.to_vec(),
                challenge: Vec::new(),
            },
        )),
        sub_sig_data: None,
        signed_message_status: None,
        request_uuid: Vec::new(),
        uuid: uuid.to_vec(),
        flags: 0,
    }
}

fn epoch_start_time(epoch_time: u32) -> SystemTime {
    SystemTime::now() - Duration::from_secs(epoch_time as u64)
}

fn epoch_bytes(info: &SessionInfo) -> [u8; EPOCH_ID_LENGTH] {
    let mut epoch = [0_u8; EPOCH_ID_LENGTH];
    let copy = info.epoch.len().min(EPOCH_ID_LENGTH);
    epoch[..copy].copy_from_slice(&info.epoch[..copy]);
    epoch
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}