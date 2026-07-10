use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use hmac::{Hmac, Mac};
use p256::PublicKey;
use p256::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::FromEncodedPoint;
use sha1::{Digest, Sha1};
use sha2::Sha256;

use crate::vehicle_command::crypto::key::PrivateKey;
use crate::vehicle_command::crypto::metadata::Metadata;
use crate::vehicle_command::error::{Result, VehicleCommandError};
use crate::vehicle_command::proto::signatures::{SignatureType, Tag};

const SHARED_KEY_SIZE: usize = 16;

type HmacSha256 = Hmac<Sha256>;

pub struct Session {
    gcm: Aes128Gcm,
    key: [u8; SHARED_KEY_SIZE],
    local_public: Vec<u8>,
}

impl Session {
    pub fn exchange(private: &PrivateKey, remote_public_bytes: &[u8]) -> Result<Self> {
        let encoded = p256::EncodedPoint::from_bytes(remote_public_bytes)
            .map_err(|_| VehicleCommandError::Crypto("invalid public key encoding".into()))?;
        let remote = Option::<PublicKey>::from(PublicKey::from_encoded_point(&encoded))
            .ok_or_else(|| VehicleCommandError::Crypto("invalid public key".into()))?;

        let shared = diffie_hellman(private.secret().to_nonzero_scalar(), remote.as_affine());
        let shared_secret = shared.raw_secret_bytes();
        let digest = Sha1::digest(shared_secret);
        let mut key = [0_u8; SHARED_KEY_SIZE];
        key.copy_from_slice(&digest[..SHARED_KEY_SIZE]);

        let gcm = Aes128Gcm::new_from_slice(&key)
            .map_err(|err| VehicleCommandError::Crypto(err.to_string()))?;

        Ok(Self {
            gcm,
            key,
            local_public: private.public_bytes().to_vec(),
        })
    }

    pub fn local_public_bytes(&self) -> &[u8] {
        &self.local_public
    }

    fn subkey(&self, label: &[u8]) -> Vec<u8> {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.key).expect("hmac key");
        mac.update(label);
        mac.finalize().into_bytes().to_vec()
    }

    pub fn new_hmac(&self, label: &str) -> HmacSha256 {
        <HmacSha256 as Mac>::new_from_slice(&self.subkey(label.as_bytes())).expect("hmac key")
    }

    pub fn session_info_hmac(
        &self,
        id: &[u8],
        challenge: &[u8],
        encoded_info: &[u8],
    ) -> Result<Vec<u8>> {
        let mut meta = Metadata::with_hmac(self.new_hmac("session info"));
        meta.add(Tag::SignatureType, &[SignatureType::Hmac as u8])?;
        meta.add(Tag::Personalization, id)?;
        meta.add(Tag::Challenge, challenge)?;
        Ok(meta.checksum(encoded_info))
    }

    pub fn encrypt(
        &self,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let nonce_bytes = rand::random::<[u8; 12]>();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext_with_tag = self
            .gcm
            .encrypt(
                nonce,
                aes_gcm::aead::Payload {
                    msg: plaintext,
                    aad: associated_data,
                },
            )
            .map_err(|err| VehicleCommandError::Crypto(err.to_string()))?;
        let tag_len = 16;
        let split = ciphertext_with_tag.len().saturating_sub(tag_len);
        let ciphertext = ciphertext_with_tag[..split].to_vec();
        let tag = ciphertext_with_tag[split..].to_vec();
        Ok((nonce_bytes.to_vec(), ciphertext, tag))
    }

    pub fn decrypt(
        &self,
        nonce: &[u8],
        ciphertext: &[u8],
        associated_data: &[u8],
        tag: &[u8],
    ) -> Result<Vec<u8>> {
        let mut ct_and_tag = ciphertext.to_vec();
        ct_and_tag.extend_from_slice(tag);
        let nonce = Nonce::from_slice(nonce);
        self.gcm
            .decrypt(
                nonce,
                aes_gcm::aead::Payload {
                    msg: &ct_and_tag,
                    aad: associated_data,
                },
            )
            .map_err(|err| VehicleCommandError::Crypto(err.to_string()))
    }
}