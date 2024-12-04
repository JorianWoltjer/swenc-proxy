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

### Notes

Methods of intercepting per request type:

|              | Background     | Navigation          |
| ------------ | -------------- | ------------------- |
| relative     | Service Worker | auto baseURI        |
| same-origin  | Service Worker | SW origin = baseURI |
| cross-origin | Service Worker | /cross-origin?url=  |

* Service Workers can only intercept background requests or same-origin navigations
