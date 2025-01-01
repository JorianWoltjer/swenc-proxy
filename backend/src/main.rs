use std::{collections::HashMap, fs::read_to_string, sync::Arc};

use axum::{
    body::{Body, Bytes},
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use axum_extra::response::JavaScript;
use base64::{prelude::BASE64_STANDARD, Engine};
use futures_util::SinkExt;
use http::{HeaderMap, HeaderName, StatusCode};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use shared::{derive_key, EncryptionCodec, ProxyRequest};
use tokio::net::TcpListener;
use tokio_util::{codec::FramedWrite, io::ReaderStream};
use tower_http::services::ServeDir;

const BIND: &str = "0.0.0.0:8000";

lazy_static::lazy_static! {
    static ref COOKIE_DOMAIN_RE: Regex = Regex::new(r"(?i)(;\s*domain=)[a-z0-9.-]+").unwrap();
}

#[derive(Clone)]
struct AppState {
    client: Arc<Client>,
    keystore: HashMap<String, [u8; 32]>,
}

#[derive(Deserialize)]
struct KeyQuery {
    key: String,
}

fn force_https(url: &str) -> String {
    // Force HTTPS, this disallows some HTTP-only sites but fixes Mixed Content issues
    if url.starts_with("http://") {
        url.replacen("http://", "https://", 1)
    } else {
        url.to_string()
    }
}

async fn check_key(
    State(state): State<AppState>,
    Query(KeyQuery { key }): Query<KeyQuery>,
) -> StatusCode {
    if state.keystore.contains_key(&key) {
        StatusCode::OK
    } else {
        StatusCode::FORBIDDEN
    }
}

async fn proxy(
    axum_headers: HeaderMap,
    State(state): State<AppState>,
    Query(KeyQuery { key }): Query<KeyQuery>,
    body: Bytes,
) -> impl IntoResponse {
    // key= parameter is a fingerprint, look it up in the keystore
    if !state.keystore.contains_key(&key) {
        return (http::StatusCode::FORBIDDEN, HeaderMap::new(), Body::empty());
    }
    let key = state.keystore[&key];

    let mut codec = EncryptionCodec::new(key);
    let decrypted = codec.decode_once(&body);
    let request: ProxyRequest = rmp_serde::from_slice(&decrypted).unwrap();

    let url = force_https(&request.url);
    let mut headers = HeaderMap::new();
    for (key, value) in request.headers {
        headers.insert(
            HeaderName::try_from(key.as_str()).unwrap(),
            value.parse().unwrap(),
        );
    }
    // Cookies can't be passed by JavaScript, so get it from the automatic Cookie header
    if let Some(cookie) = axum_headers.get("cookie") {
        headers.insert("cookie", cookie.clone());
    }
    // Domain used later on to rescope cookies
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
    let mut response = state
        .client
        .request(method, &url)
        .headers(headers)
        .body(body.unwrap_or_default())
        .send()
        .await
        .unwrap();

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
            | "content-security-policy"
            | "content-security-policy-report-only"
            | "x-frame-options" => continue,
            // Modify `Location` header because fetch() follows redirects
            "location" => real_key = "x-location".parse().unwrap(),
            // Modify `Content-Length` header to hint clients about the real length (axum uses chunked encoding)
            "content-length" => real_key = "x-content-length".parse().unwrap(),
            // Modify cookies to be scoped to the proxy domain
            "set-cookie" => {
                value = COOKIE_DOMAIN_RE
                    .replace_all(value.to_str().unwrap(), format!("${{1}}{}", axum_domain))
                    .parse()
                    .unwrap();
            }
            _ => {}
        }
        headers.append(real_key, value);
    }

    // Stream response body while decrypting
    let (writer, reader) = tokio::io::duplex(64);
    let reader = ReaderStream::new(reader);
    tokio::spawn(async move {
        let codec = EncryptionCodec::new(key);
        let mut writer = FramedWrite::new(writer, codec);
        while let Some(chunk) = response.chunk().await.unwrap() {
            writer.send(chunk.to_vec()).await.unwrap();
        }
    });

    (status_code, headers, Body::from_stream(reader))
}

#[tokio::main]
async fn main() {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let keys = read_to_string("keys.txt").unwrap();
    let keystore = keys
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|key| {
            // First derive to get a good key
            let key = derive_key(key.as_bytes());
            // Derive again to get a hash safe for sharing
            let fingerprint = sha256::digest(&key);
            (fingerprint, key)
        })
        .collect::<HashMap<_, _>>();

    println!("Loaded {} keys", keystore.len());

    let state = AppState {
        client: Arc::new(client),
        keystore,
    };

    let listener = TcpListener::bind(BIND).await.unwrap();
    println!("Listening on http://{BIND}");

    let router = Router::new()
        .nest(
            "/swenc-proxy",
            Router::new()
                .route("/proxy/", post(proxy))
                .route("/proxy/:filename", post(proxy))
                .route("/check", get(check_key))
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
        .route(
            "/favicon.ico",
            get(|| async {
                (
                    [("Content-Type", "image/x-icon")],
                    include_bytes!("../../frontend/public/favicon.ico"),
                )
            }),
        )
        .fallback(|| async { Redirect::temporary("/swenc-proxy/") })
        .with_state(state);

    axum::serve(listener, router).await.unwrap();
}
