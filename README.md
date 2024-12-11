# Service Worker Encrypted Proxy

> [!NOTE]  
> Due to some missing cutting-edge browser features, this application only works on Chromium-based browsers, not Firefox.

## Usage

1. Enter the **encryption key** configured on the server. This application is only usable with knowledge of the key.
2. **Visit** a URL in the proxy, browse around
3. When you want to **exit**, close all tabs to this application, then open it again

## Backend

```sh
cargo run --release
```

## Frontend

Requirements:

* https://rustwasm.github.io/wasm-pack/installer/

Building (inside `frontend/`):

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

## Known Issues

##### Cloudflare challenges

Cloudflare challenges don't work because an iframe with origin `about:blank` makes an external request. Unforunately, [an open Chromium issue](https://issues.chromium.org/issues/41411856) says that Service Workers cannot capture such requests even though the parent is same-origin with the frame. The following is a proof-of-concept:

```html
<iframe srcdoc="<script>fetch('https://example.com')</script>"></iframe>
```

##### Multiple origins in different tabs

Due to only one Service Worker have a single `targetOrigin` it keeps track of, only one origin can be open at the same time when having multiple tabs of the proxy open. This issue is theoretically solvable and may be implemented in the future.

---

If you encounter any other issues, please let me know in an [Issue](https://github.com/JorianWoltjer/swenc-proxy/issues/new). Send me the URL you are trying to visit, and a screenshot of the error would be helpful.
