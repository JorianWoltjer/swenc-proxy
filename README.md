# Service Worker Encrypted Proxy

**Bypass SSL-intercepting firewalls in-browser using a Service Worker that encrypts all requests and responses!**

https://github.com/user-attachments/assets/e0b2c066-fd5f-40f4-a5e6-d0aae7f0032c

> [!NOTE]  
> Due to some missing cutting-edge browser features, this application only works on Chromium-based browsers, not Firefox.

## Setup

This application is only usable with a valid "encryption key". These are saved in a [`keys.txt`](backend/keys.txt) file separated by newlines, and should be changed to something completely random.

> [!WARNING]  
> This application can request any HTTPS url that you request through it. If you share a secret key with anyone, **be careful of misuse** by requesting URLs into the internal network of your proxy server.

Then, the `frontend/` should be built with the following command:

(requirement: https://rustwasm.github.io/wasm-pack/installer/)

```sh
wasm-pack build --no-pack --target web
```

Finally, you can build the `backend/` with the following command:

(requirement: https://www.rust-lang.org/tools/install)

```sh
cargo build --release
```

This builds a `./target/release/backend` binary in the main directory. By default, it listens on `0.0.0.0:8000` but this can be changed with the [`BIND`](./backend/src/main.rs#L22) variable. When you run the binary, it will open up the HTTP server and start accepting connections. You should see the UI when visiting http://localhost:8000.

### Usage

1. Enter a valid **encryption key** configured on the server.
2. **Visit** a URL in the proxy, browse around
3. When you want to **exit**, close all tabs to this application, then open it again (or go back to http://localhost/swenc-proxy/ manually)

## Technical Details

### How does the encryption work?

The chosen encryption key is always used, to eliminate the need for an impossible key exchange when even TLS can't be trusted (in the case of SSL-intercepting firewalls).

All requests are AES-GCM encrypted before being sent to the `POST /swenc-proxy/proxy` endpoint. This encryption happens in WebAssembly compiled from Rust. See [`frontend/src/lib.rs`](frontend/src/lib.rs#L95) for the implementation.

Responses are also encrypted with AES-GCM in the same way, but are streamed to make the experience a lot smoother and avoid high memory usage for large downloads. See [`backend/src/main.rs`](backend/src/main.rs#L145) for this implementation.

### How are requests intercepted?

Service Workers can intercept requests in their scope using the [`fetch`](https://developer.mozilla.org/en-US/docs/Web/API/ServiceWorkerGlobalScope/fetch_event) event. They can currently only intercept any background requests or same-origin navigations.

Methods of intercepting per request type:

|              | Background                    | Navigation                    |
| ------------ | ----------------------------- | ----------------------------- |
| relative     | Service Worker, change origin | Service Worker, change origin |
| same-origin  | Service Worker                | Service Worker                |
| cross-origin | Service Worker                | /url?url=                     |

As seen above, just adding a service worker to capture the `fetch` event won't cover all possible ways a user can navigate the page, so another script named [`prison.js`](frontend/public/prison.js) is inserted on every single HTML page to replace `<a>` tags and `<iframe>`s.

## Known Issues

##### Cloudflare challenges

Cloudflare challenges don't work because an iframe with origin `about:blank` makes an external request. Unforunately, [an open Chromium issue](https://issues.chromium.org/issues/41411856) says that Service Workers cannot capture such requests even though the parent is same-origin with the frame. The following is a proof-of-concept:

```html
<iframe srcdoc="<script>fetch('https://example.com')</script>"></iframe>
```

##### Multiple origins in different tabs

Due to only one Service Worker have a single `targetOrigin` it keeps track of, only one origin can be open at the same time when having multiple tabs of the proxy open. This issue is theoretically solvable and may be implemented in the future.

##### Missing Firefox support

Due to Firefox [not having implemented `import` syntax in service workers yet](https://bugzilla.mozilla.org/show_bug.cgi?id=1360870), this application can't load the service worker as it uses this syntax for importing the WebAssembly module ([here](frontend/public/worker.js#L1)).

```js
// worker.js
import { decrypt_stream, derive_key, serialize_proxy_request, sha256 } from '/swenc-proxy/utils.js';
...
```

Apart from this, [the `navigation` event is also missing a Firefox implementation](https://bugzilla.mozilla.org/show_bug.cgi?id=1890755). This event is used [here](frontend/public/prison.js#L103) to capture an otherwise impossible navigation using `window.location = ...`.

```js
// prison.js
navigation.addEventListener("navigate", (event) => {
  ...
}
```

When these features are implemented, I will spend some time figuring out other issues that I can fix on Firefox and hopefully make it work for both browsers.

---

If you encounter any other issues, please let me know in an [Issue](https://github.com/JorianWoltjer/swenc-proxy/issues/new). Send me the URL you are trying to visit, and a screenshot of the error would be helpful.
