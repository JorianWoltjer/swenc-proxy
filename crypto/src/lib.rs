use std::io;

use aes_gcm::{
    AeadCore, Aes256Gcm, KeyInit, Nonce,
    aead::{self, Aead, OsRng},
};
use argon2::Argon2;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::mpsc,
};
use tokio_util::bytes::Bytes;

const SALT: &[u8] = b"wasm-dl-salt";

pub fn derive_key(password: &str) -> [u8; 32] {
    let mut key = [0; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), SALT, &mut key)
        .unwrap();
    key
}

// TODO: implement FramedRead (https://docs.rs/tokio-util/latest/tokio_util/codec/index.html)
pub struct EncryptedChunk {
    pub nonce: aead::Nonce<Aes256Gcm>,
    pub ciphertext: Vec<u8>,
}
impl EncryptedChunk {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&(self.ciphertext.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let nonce = *Nonce::from_slice(&bytes[..12]);
        let ciphertext = bytes[16..].to_vec();

        Self { nonce, ciphertext }
    }

    pub async fn from_reader<R: AsyncRead + Unpin>(mut reader: R) -> Result<Self, io::Error> {
        let mut nonce = [0; 12];
        reader.read_exact(&mut nonce).await?;
        let nonce = *Nonce::from_slice(&nonce);
        let mut len = [0; 4];
        reader.read_exact(&mut len).await?;
        let len = u32::from_le_bytes(len) as usize;
        let mut ciphertext = vec![0; len];
        reader.read_exact(&mut ciphertext).await?;

        Ok(Self { nonce, ciphertext })
    }

    pub fn decrypt(&self, key: &[u8]) -> Vec<u8> {
        let cipher = aes_gcm::Aes256Gcm::new_from_slice(key).unwrap();
        cipher.decrypt(&self.nonce, &*self.ciphertext).unwrap()
    }

    pub fn encrypt(key: &[u8], plaintext: &[u8]) -> Self {
        let cipher = aes_gcm::Aes256Gcm::new_from_slice(key).unwrap();
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, plaintext).unwrap();

        Self { nonce, ciphertext }
    }

    pub async fn encrypt_reader<R: AsyncRead + Unpin>(
        mut reader: R,
        key: &[u8],
        tx: mpsc::UnboundedSender<Result<Vec<u8>, io::Error>>,
    ) {
        let mut buffer = [0; 4096];
        while let Ok(len) = reader.read(&mut buffer).await {
            if len == 0 {
                break;
            }
            let chunk = Self::encrypt(key, &buffer[..len]);
            tx.send(Ok(chunk.to_bytes())).unwrap();
            buffer = [0; 4096];
        }
    }
}

pub async fn decrypt_stream<R: AsyncRead + Unpin>(
    mut reader: R,
    key: &[u8],
    tx: mpsc::Sender<Bytes>,
) {
    while let Ok(chunk) = EncryptedChunk::from_reader(&mut reader).await {
        let plaintext = chunk.decrypt(key);
        tx.send(Bytes::from(plaintext)).await.unwrap();
    }
}
