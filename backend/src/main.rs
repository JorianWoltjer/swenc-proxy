use std::sync::Arc;

use aes_gcm::{
    AeadCore, Aes256Gcm, KeyInit,
    aead::{Aead, Nonce, OsRng},
};
use argon2::Argon2;
use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
};
use reqwest::Client;
use serde::Deserialize;
use tokio::{net::TcpListener, sync::mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower_http::services::ServeDir;

const KEY: &[u8] = b"secret";
const SALT: &[u8] = b"wasm-dl-salt";

#[derive(Clone)]
struct AppState {
    client: Arc<Client>,
    cipher: Aes256Gcm,
}

#[derive(Deserialize)]
struct ProxyQuery {
    url: String,
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
    Query(query): Query<ProxyQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let url = query.url;
    let client = state.client.clone();

    let mut response = client.get(&url).send().await.unwrap();
    let (tx, rx) = mpsc::unbounded_channel::<Result<Vec<u8>, std::io::Error>>();

    tokio::spawn(async move {
        while let Some(chunk) = response.chunk().await.unwrap() {
            let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
            let ciphertext = state.cipher.encrypt(&nonce, &*chunk).unwrap();
            tx.send(Ok(EncryptedChunk { nonce, ciphertext }.to_bytes()))
                .unwrap();
        }
    });

    Body::from_stream(UnboundedReceiverStream::new(rx))
}

#[tokio::main]
async fn main() {
    let client = Client::new();
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
        .route("/proxy", get(proxy))
        .nest_service("/pkg", ServeDir::new("../frontend/pkg"))
        .fallback_service(
            ServeDir::new("../frontend/public").append_index_html_on_directories(true),
        )
        .with_state(state);

    axum::serve(listener, router).await.unwrap();
}
