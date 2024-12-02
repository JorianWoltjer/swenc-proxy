mod utils;

use aes_gcm::{aead::Aead, KeyInit, Nonce};
use argon2::Argon2;
use futures::StreamExt;
use tokio::io::AsyncReadExt;
use tokio_util::bytes::Bytes;
use tokio_util::io::StreamReader;
use utils::set_panic_hook;
use wasm_bindgen::prelude::*;
use wasm_streams::ReadableStream;
use web_sys::{js_sys::Uint8Array, ReadableStreamDefaultController};

const SALT: &[u8] = b"wasm-dl-salt";

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen]
pub fn derive_key(password: &str) -> Vec<u8> {
    set_panic_hook();

    log(&format!("derive_key: {}", password));

    let mut key = [0; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), SALT, &mut key)
        .unwrap();

    key.to_vec()
}

#[wasm_bindgen]
pub async fn decrypt(
    stream: web_sys::ReadableStream,
    writer: ReadableStreamDefaultController,
    key: &[u8],
) {
    // TODO: error handling on wrong key
    set_panic_hook();
    log("decrypt()");

    let stream = ReadableStream::from_raw(stream).into_stream();
    let stream = stream.map(|value| {
        let value = value.map_err(|err| {
            std::io::Error::new(std::io::ErrorKind::Other, err.as_string().unwrap())
        })?;
        let value = Uint8Array::new(&value);
        let value = value.to_vec();
        Ok::<_, std::io::Error>(Bytes::from(value))
    });
    let mut reader = StreamReader::new(stream);
    // TODO: BufReader?

    loop {
        let mut nonce = [0; 12];
        if reader.read_exact(&mut nonce).await.is_err() {
            break;
        }
        // log(&format!("nonce: {:?}", nonce));
        let nonce = Nonce::from_slice(&nonce);
        let mut len = [0; 4];
        reader.read_exact(&mut len).await.unwrap();
        let len = u32::from_le_bytes(len) as usize;
        log(&format!("len: {:?}", len));
        let mut ciphertext = vec![0; len];
        reader.read_exact(&mut ciphertext).await.unwrap();
        // log(&format!("ciphertext: {:?}", ciphertext));

        let cipher = aes_gcm::Aes256Gcm::new_from_slice(key).unwrap();
        let plaintext = cipher.decrypt(nonce, &*ciphertext).unwrap();
        // log(&format!("plaintext: {:?}", plaintext));

        // TODO: check if writer already closed
        writer
            .enqueue_with_chunk(unsafe { &Uint8Array::new(&Uint8Array::view(&plaintext)) })
            .unwrap();
    }

    log("done!");
    writer.close().unwrap();
}
