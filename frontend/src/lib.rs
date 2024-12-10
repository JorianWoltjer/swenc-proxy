mod utils;

use std::io;

use futures::StreamExt;
use shared::{EncryptionCodec, ProxyRequest};
use tokio_util::{bytes::Bytes, codec::FramedRead, io::StreamReader};
use utils::set_panic_hook;
use wasm_bindgen::prelude::*;
use wasm_streams::ReadableStream;
use web_sys::{
    ReadableStreamDefaultController,
    js_sys::{self, Uint8Array},
};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    pub type JsProxyRequest;

    #[wasm_bindgen(method, getter)]
    fn url(this: &JsProxyRequest) -> String;

    #[wasm_bindgen(method, getter)]
    fn method(this: &JsProxyRequest) -> String;

    #[wasm_bindgen(method, getter)]
    fn headers(this: &JsProxyRequest) -> js_sys::Array;

    #[wasm_bindgen(method, getter)]
    fn body(this: &JsProxyRequest) -> Option<Vec<u8>>;
}

#[wasm_bindgen]
pub fn derive_key(password: &str) -> Vec<u8> {
    set_panic_hook();
    log(&format!("derive_key: {}", password));
    shared::derive_key(password.as_bytes()).to_vec()
}

#[wasm_bindgen]
pub async fn decrypt_stream(
    stream: web_sys::ReadableStream,
    writer: ReadableStreamDefaultController,
    key: &[u8],
) {
    // TODO: error handling on wrong key
    set_panic_hook();
    log("decrypt_stream()");

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
    log("decrypt_stream() done!");
    writer.close().unwrap();
}

impl From<JsProxyRequest> for ProxyRequest {
    fn from(js_request: JsProxyRequest) -> Self {
        let url = js_request.url();
        let method = js_request.method();
        let headers = js_request
            .headers()
            .into_iter()
            .map(|header| {
                let header = js_sys::Array::from(&header);
                (
                    header.get(0).as_string().unwrap(),
                    header.get(1).as_string().unwrap(),
                )
            })
            .collect();

        let body = js_request.body();
        ProxyRequest {
            url,
            method,
            headers,
            body,
        }
    }
}

#[wasm_bindgen]
pub fn serialize_proxy_request(object: JsProxyRequest, key: &[u8]) -> js_sys::Uint8Array {
    let request: ProxyRequest = object.into();
    let serialized = bincode::serialize(&request).unwrap();

    let mut codec = EncryptionCodec::new(key.try_into().unwrap());
    let encrypted = codec.encode_once(&serialized);

    unsafe { js_sys::Uint8Array::new(&js_sys::Uint8Array::view(&encrypted)) }
}
