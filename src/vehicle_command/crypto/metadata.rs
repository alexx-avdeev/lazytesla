use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};

use super::super::proto::signatures::Tag;
use crate::vehicle_command::error::{Result, VehicleCommandError};

const TAG_END: u8 = 255;

enum Hasher {
    Sha256(Sha256),
    Sha512(Sha512),
    Hmac(Hmac<Sha256>),
}

impl Hasher {
    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha256(h) => h.update(data),
            Self::Sha512(h) => h.update(data),
            Self::Hmac(h) => h.update(data),
        }
    }

    fn finalize(self) -> Vec<u8> {
        match self {
            Self::Sha256(h) => h.finalize().to_vec(),
            Self::Sha512(h) => h.finalize().to_vec(),
            Self::Hmac(h) => h.finalize().into_bytes().to_vec(),
        }
    }
}

pub struct Metadata {
    context: Hasher,
    last: i32,
}

impl Metadata {
    pub fn new_sha256() -> Self {
        Self::with_hasher(Hasher::Sha256(Sha256::new()))
    }

    #[cfg(test)]
    pub fn new_sha512() -> Self {
        Self::with_hasher(Hasher::Sha512(Sha512::new()))
    }

    pub fn with_hmac(hmac: Hmac<Sha256>) -> Self {
        Self::with_hasher(Hasher::Hmac(hmac))
    }

    fn with_hasher(context: Hasher) -> Self {
        Self { context, last: -1 }
    }

    pub fn add(&mut self, tag: Tag, value: &[u8]) -> Result<()> {
        let tag_i = tag as i32;
        if tag_i < self.last {
            return Err(VehicleCommandError::Crypto(
                "metadata items need to be added in increasing tag order".into(),
            ));
        }
        if value.is_empty() {
            return Ok(());
        }
        if value.len() > 255 {
            return Err(VehicleCommandError::Crypto(
                "metadata fields can't be more than 255 bytes long".into(),
            ));
        }
        self.last = tag_i;
        self.context.update(&[tag as u8]);
        self.context.update(&[value.len() as u8]);
        self.context.update(value);
        Ok(())
    }

    pub fn add_u32(&mut self, tag: Tag, value: u32) -> Result<()> {
        self.add(tag, &value.to_be_bytes())
    }

    pub fn checksum(mut self, message: &[u8]) -> Vec<u8> {
        self.context.update(&[TAG_END]);
        self.context.update(message);
        self.context.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_command::proto::signatures::Tag;

    #[test]
    fn checksum_matches_go_test_vector() {
        let items = [
            (Tag::SignatureType, vec![0x05_u8]),
            (Tag::Domain, vec![0x02]),
            (Tag::Personalization, b"testVIN".to_vec()),
            (
                Tag::Epoch,
                vec![
                    0xaa, 0xda, 0x92, 0x8a, 0x4f, 0x21, 0x5f, 0x55, 0xf9, 0xe6, 0xe4, 0x5e, 0x66,
                    0xb6, 0x52, 0x1e,
                ],
            ),
            (Tag::ExpiresAt, vec![0x00, 0x00, 0x0e, 0x74]),
            (Tag::Counter, vec![0x00, 0x00, 0x05, 0x3a]),
        ];
        let expected = [
            0xab, 0xab, 0x04, 0xd8, 0x04, 0x49, 0x98, 0x13, 0x38, 0x2e, 0xfd, 0x74, 0xa0, 0x67,
            0x91, 0xce, 0x2d, 0xe7, 0x77, 0x43, 0x96, 0x03, 0x24, 0x6d, 0xfb, 0xaa, 0x83, 0x92,
            0xca, 0x05, 0x86, 0x8e,
        ];

        let mut meta = Metadata::new_sha256();
        for (tag, value) in items {
            meta.add(tag, &value).expect("add metadata");
        }
        let computed = meta.checksum(&[]);
        assert_eq!(computed, expected);
    }

    #[test]
    fn sha512_checksum_matches_go_test_vector() {
        let items = [
            (Tag::SignatureType, vec![0x05_u8]),
            (Tag::Domain, vec![0x02]),
            (Tag::Personalization, b"testVIN".to_vec()),
            (
                Tag::Epoch,
                vec![
                    0xaa, 0xda, 0x92, 0x8a, 0x4f, 0x21, 0x5f, 0x55, 0xf9, 0xe6, 0xe4, 0x5e, 0x66,
                    0xb6, 0x52, 0x1e,
                ],
            ),
            (Tag::ExpiresAt, vec![0x00, 0x00, 0x0e, 0x74]),
            (Tag::Counter, vec![0x00, 0x00, 0x05, 0x3a]),
        ];
        let expected = [
            0xdf, 0x4a, 0x60, 0xe0, 0x3f, 0xd4, 0xf7, 0x1a, 0x83, 0xe6, 0xb5, 0x6c, 0xcf, 0x27,
            0xcc, 0xf3, 0x90, 0x26, 0x9b, 0xa3, 0xfc, 0xcf, 0xaf, 0xd9, 0xcb, 0x3a, 0x09, 0x25,
            0xfc, 0x36, 0x84, 0x38, 0x66, 0xb4, 0x32, 0x66, 0x55, 0xf1, 0xc9, 0xd5, 0x39, 0xc7,
            0xff, 0xc6, 0xf3, 0x31, 0xba, 0x69, 0x3e, 0x1c, 0x62, 0xd2, 0x37, 0xcb, 0x6c, 0xb5,
            0xd9, 0xe6, 0x04, 0x39, 0xf9, 0x8f, 0x22, 0x83,
        ];

        let mut meta = Metadata::new_sha512();
        for (tag, value) in items {
            meta.add(tag, &value).expect("add metadata");
        }
        let computed = meta.checksum(&[]);
        assert_eq!(computed, expected);
    }
}