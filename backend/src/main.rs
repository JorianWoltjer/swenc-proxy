use std::{io, sync::Arc};

use axum::{
    Router,
    body::Body,
    extract::{Json, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use axum_extra::response::JavaScript;
use base64::{Engine, prelude::BASE64_STANDARD};
use crypto::{EncryptedChunk, derive_key};
use futures_util::TryStreamExt;
use http::{HeaderMap, HeaderName};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use tokio::{net::TcpListener, sync::mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::io::StreamReader;
use tower_http::services::ServeDir;

const KEY: &str = "secret";

lazy_static::lazy_static! {
    static ref COOKIE_DOMAIN_RE: Regex = Regex::new(r"(?i)(;\s*domain=)[a-z0-9.-]+").unwrap();
}

#[derive(Clone)]
struct AppState {
    client: Arc<Client>,
    key: [u8; 32],
}

// TODO: encrypt this too with same key
#[derive(Deserialize)]
struct ProxyRequest {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
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
    let axum_domain = axum_headers
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
    let response = state
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

    let mut headers = HeaderMap::new();
    let mut last_key = None;
    for (key, mut value) in response_headers {
        let mut real_key = if key.is_none() {
            last_key.clone().unwrap()
        } else {
            last_key = key.clone();
            key.unwrap()
        };
        match real_key.as_str() {
            // Skip these special response headers
            "transfer-encoding"
            | "content-length"
            | "content-security-policy"
            | "content-security-policy-report-only"
            | "x-frame-options" => continue,
            // Modify `Location` header because fetch() follows redirects
            "location" => real_key = "x-location".parse().unwrap(),
            // Modify cookies to be scoped to the proxy domain
            "set-cookie" => {
                value = COOKIE_DOMAIN_RE
                    .replace_all(value.to_str().unwrap(), format!("$1{}", axum_domain))
                    .parse()
                    .unwrap();
            }
            _ => {}
        }
        headers.insert(real_key, value);
    }

    let reader = StreamReader::new(response.bytes_stream().map_err(io::Error::other));
    tokio::spawn(async move {
        EncryptedChunk::encrypt_reader(reader, &state.key, tx).await;
    });

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
    let key = derive_key(KEY);

    let state = AppState {
        client: Arc::new(client),
        key,
    };

    // TODO: host on separate VPS due to SSRF concerns
    let listen_address = "0.0.0.0:8000";
    let listener = TcpListener::bind(listen_address).await.unwrap();
    println!("Listening on http://{listen_address}");

    // TODO: also make CSRF proof with fetch-mode
    let router = Router::new()
        .nest(
            "/swenc-proxy",
            Router::new()
                .route("/proxy/", post(proxy))
                .route("/proxy/:filename", post(proxy))
                .nest_service("/pkg", ServeDir::new("../frontend/pkg"))
                .fallback_service(ServeDir::new("../frontend/public")),
        )
        .route(
            "/swenc-proxy/", // To handle no path
            get(|| async { Html(include_str!("../../frontend/public/index.html")) }),
        )
        .route(
            "/swenc-proxy-sw.js", // Needs to be in the root directory
            get(|| async { JavaScript(include_str!("../../frontend/public/worker.js")) }),
        )
        .route("/", get(|| async { Redirect::temporary("/swenc-proxy/") }))
        .with_state(state);

    axum::serve(listener, router).await.unwrap();
}
