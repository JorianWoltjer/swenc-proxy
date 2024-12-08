use std::io;

use aes_gcm::{
    AeadCore, Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng},
};
use argon2::Argon2;
use tokio_util::{
    bytes::{Buf, BufMut, BytesMut},
    codec::{Decoder, Encoder},
};

const SALT: &[u8] = b"wasm-dl-salt";
const HEADER_SIZE: usize = 12 + 4; // nonce + length

pub fn derive_key(password: &str) -> [u8; 32] {
    let mut key = [0; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), SALT, &mut key)
        .unwrap();
    key
}

pub struct EncryptionCodec {
    pub cipher: Aes256Gcm,
}
impl EncryptionCodec {
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            cipher: Aes256Gcm::new_from_slice(&key).unwrap(),
        }
    }
}
impl Decoder for EncryptionCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < HEADER_SIZE {
            // Not enough bytes to read nonce and length
            return Ok(None);
        }
        let nonce = *Nonce::from_slice(&src[..12]);
        let len =
            u32::from_le_bytes(src[HEADER_SIZE - 4..HEADER_SIZE].try_into().unwrap()) as usize;
        if src.len() < HEADER_SIZE + len {
            // Not enough bytes to read the whole chunk
            return Ok(None);
        }
        let ciphertext = &src[HEADER_SIZE..HEADER_SIZE + len];
        let plaintext = self.cipher.decrypt(&nonce, ciphertext).unwrap();
        src.advance(HEADER_SIZE + len);

        Ok(Some(plaintext))
    }
}
impl Encoder<Vec<u8>> for EncryptionCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self.cipher.encrypt(&nonce, &*item).unwrap();
        dst.reserve(HEADER_SIZE + ciphertext.len());
        dst.put_slice(nonce.as_ref());
        dst.put_u32_le(ciphertext.len() as u32);
        dst.put_slice(&ciphertext);

        Ok(())
    }
}
