use std::sync::Arc;

use aes_gcm::{
    AeadCore, Aes256Gcm, KeyInit,
    aead::{Aead, Nonce, OsRng},
};
use argon2::Argon2;
use axum::{
    Router,
    body::Body,
    extract::{Json, State},
    response::IntoResponse,
    routing::{get, post},
};
use base64::{Engine, prelude::BASE64_STANDARD};
use http::{HeaderMap, HeaderName};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use tokio::{net::TcpListener, sync::mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower_http::services::ServeDir;

const KEY: &[u8] = b"secret";
const SALT: &[u8] = b"wasm-dl-salt";

lazy_static::lazy_static! {
    static ref COOKIE_DOMAIN_RE: Regex = Regex::new(r"(?i)(;\s*domain=)[a-z0-9.-]+").unwrap();
}

#[derive(Clone)]
struct AppState {
    client: Arc<Client>,
    cipher: Aes256Gcm,
}

// TODO: encrypt this too with same key
#[derive(Deserialize)]
struct ProxyRequest {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

struct EncryptedChunk {
    nonce: Nonce<Aes256Gcm>,
    ciphertext: Vec<u8>,
}
impl EncryptedChunk {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&(self.ciphertext.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }
}

async fn proxy(
    axum_headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<ProxyRequest>,
) -> impl IntoResponse {
    let url = request.url;
    let mut headers = HeaderMap::new();
    for (key, value) in request.headers {
        headers.insert(
            HeaderName::try_from(key.as_str()).unwrap(),
            value.parse().unwrap(),
        );
    }
    if let Some(cookie) = axum_headers.get("cookie") {
        headers.insert("cookie", cookie.clone());
    }
    let domain = axum_headers
        .get("host")
        .unwrap()
        .to_str()
        .unwrap()
        .split(':')
        .next()
        .unwrap();
    let method = reqwest::Method::from_bytes(request.method.as_bytes()).unwrap();
    let body = request
        .body
        .map(|body| BASE64_STANDARD.decode(body).unwrap());

    println!("Proxying: {}", url);
    let mut response = state
        .client
        .request(method, &url)
        .headers(headers)
        .body(body.unwrap_or_default())
        .send()
        .await
        .unwrap();
    let (tx, rx) = mpsc::unbounded_channel::<Result<Vec<u8>, std::io::Error>>();

    let response_headers = response.headers().clone();
    let status_code = response.status();

    tokio::spawn(async move {
        while let Some(chunk) = response.chunk().await.unwrap() {
            let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
            let ciphertext = state.cipher.encrypt(&nonce, &*chunk).unwrap();
            tx.send(Ok(EncryptedChunk { nonce, ciphertext }.to_bytes()))
                .unwrap();
        }
    });

    let mut headers = HeaderMap::new();
    let mut last_key = None;
    for (key, mut value) in response_headers {
        if let Some(mut key) = key {
            last_key = Some(key.clone());
            match key.as_str() {
                "transfer-encoding"
                | "content-length"
                | "content-security-policy"
                | "content-security-policy-report-only"
                | "x-frame-options" => continue,
                "location" => key = "x-location".parse().unwrap(),
                "set-cookie" => {
                    value = COOKIE_DOMAIN_RE
                        .replace_all(value.to_str().unwrap(), format!("${{1}}{}", domain))
                        .parse()
                        .unwrap();
                }
                _ => {}
            }

            headers.insert(key, value.clone());
        } else {
            headers.append(last_key.clone().unwrap(), value.clone());
        }
    }

    (
        status_code,
        headers,
        Body::from_stream(UnboundedReceiverStream::new(rx)),
    )
}

#[tokio::main]
async fn main() {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let mut key = [0; 32];
    Argon2::default()
        .hash_password_into(KEY, SALT, &mut key)
        .unwrap();

    let state = AppState {
        client: Arc::new(client),
        cipher: Aes256Gcm::new_from_slice(&key).unwrap(),
    };

    let listen_address = "0.0.0.0:8000";
    let listener = TcpListener::bind(listen_address).await.unwrap();
    println!("Listening on http://{listen_address}");

    let router = Router::new()
        .route("/proxy/", post(proxy))
        .route("/proxy/:filename", post(proxy))
        .route(
            "/dummy",
            get(|| async { "You shouldn't have gotten here..." }),
        )
        .nest_service("/pkg", ServeDir::new("../frontend/pkg"))
        .fallback_service(
            ServeDir::new("../frontend/public").append_index_html_on_directories(true),
        )
        .with_state(state);

    // TODO: check multithreading
    axum::serve(listener, router).await.unwrap();
}
