use std::fs;
use std::path::Path;

use p256::SecretKey;
use p256::elliptic_curve::pkcs8::DecodePrivateKey;
use p256::elliptic_curve::sec1::ToEncodedPoint;

use crate::vehicle_command::error::{Result, VehicleCommandError};

pub struct PrivateKey {
    secret: SecretKey,
    public_bytes: Vec<u8>,
}

impl PrivateKey {
    pub fn load(path: &Path) -> Result<Self> {
        let pem = fs::read_to_string(path).map_err(|err| {
            VehicleCommandError::InvalidKey(format!("failed to read key file: {err}"))
        })?;

        let secret = if pem.contains("BEGIN EC PRIVATE KEY") {
            SecretKey::from_sec1_pem(&pem).map_err(map_key_err)?
        } else {
            SecretKey::from_pkcs8_pem(&pem).map_err(map_key_err)?
        };

        let public_bytes = secret
            .public_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec();

        if public_bytes.len() != 65 || public_bytes[0] != 0x04 {
            return Err(VehicleCommandError::InvalidKey(
                "expected uncompressed P-256 public key".into(),
            ));
        }

        Ok(Self {
            secret,
            public_bytes,
        })
    }

    pub fn secret(&self) -> &SecretKey {
        &self.secret
    }

    pub fn public_bytes(&self) -> &[u8] {
        &self.public_bytes
    }
}

fn map_key_err(err: impl std::fmt::Display) -> VehicleCommandError {
    VehicleCommandError::InvalidKey(format!("{err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_ec_private_key_pem() {
        let path = format!("{}/config/fleet-key.pem", env!("CARGO_MANIFEST_DIR"));
        if !Path::new(&path).exists() {
            return;
        }

        let key = PrivateKey::load(Path::new(&path)).expect("load fleet key");
        assert_eq!(key.public_bytes().len(), 65);
        assert_eq!(key.public_bytes()[0], 0x04);
    }
}