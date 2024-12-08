mod utils;

use std::io;

use crypto::EncryptionCodec;
use futures::StreamExt;
use tokio_util::{bytes::Bytes, codec::FramedRead, io::StreamReader};
use utils::set_panic_hook;
use wasm_bindgen::prelude::*;
use wasm_streams::ReadableStream;
use web_sys::{ReadableStreamDefaultController, js_sys::Uint8Array};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen]
pub fn derive_key(password: &str) -> Vec<u8> {
    set_panic_hook();
    log(&format!("derive_key: {}", password));
    crypto::derive_key(password).to_vec()
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
        let value =
            value.map_err(|err| io::Error::new(io::ErrorKind::Other, err.as_string().unwrap()))?;
        let value = Uint8Array::new(&value);
        let value = value.to_vec();
        Ok::<_, io::Error>(Bytes::from(value))
    });
    let reader = StreamReader::new(stream);
    // TODO: BufReader?

    let codec = EncryptionCodec::new(key.try_into().unwrap());
    let mut reader = FramedRead::new(reader, codec);

    while let Some(chunk) = reader.next().await {
        let chunk = chunk.unwrap();
        log(&format!("len: {:?}", chunk.len()));
        unsafe {
            writer
                .enqueue_with_chunk(&Uint8Array::new(&Uint8Array::view(&chunk)))
                .unwrap();
        }
    }
}
