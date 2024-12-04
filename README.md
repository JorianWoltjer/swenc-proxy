# Service Worker Encrypted Proxy

- paste URL, send it encrypted through wasm
- stream download content and decrypt in wasm
- pre-shared key and AES-GCM

## Backend

```
cargo run
```

## Frontend

```sh
wasm-pack build --no-pack --target web
```
