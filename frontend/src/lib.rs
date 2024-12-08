mod utils;

use std::io;

use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_util::bytes::Bytes;
use tokio_util::io::StreamReader;
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
    let mut reader = StreamReader::new(stream);
    // TODO: BufReader?

    let (tx, mut rx) = mpsc::channel::<Bytes>(32);
    wasm_bindgen_futures::spawn_local(async move {
        while let Some(chunk) = rx.recv().await {
            log(&format!("len: {:?}", chunk.len()));
            unsafe {
                writer
                    .enqueue_with_chunk(&Uint8Array::new(&Uint8Array::view(&chunk)))
                    .unwrap();
            }
        }
        writer.close().unwrap();
        log("done!");
    });

    crypto::decrypt_stream(&mut reader, key, tx).await;
}
