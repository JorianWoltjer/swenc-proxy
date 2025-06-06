import { decrypt_stream, derive_key, serialize_proxy_request, sha256 } from '/swenc-proxy/utils.js';

self.addEventListener("activate", (event) => {
  event.waitUntil(clients.claim());
});

function isMetaRequest(url) {
  // Any requests to /swenc-proxy shouldn't be intercepted, except for /url for dummy requests
  return url.origin === location.origin && url.pathname.startsWith('/swenc-proxy') &&
    !url.pathname.startsWith('/swenc-proxy/url');
}

self.addEventListener('fetch', async (event) => {
  const url = new URL(event.request.url);

  // Don't intercept meta or non-HTTP requests
  if (isMetaRequest(url) ||
    (url.protocol !== 'http:' && url.protocol !== 'https:')) {

    event.respondWith(fetch(event.request));
    return;
  }

  // Start proxying
  event.respondWith(fetchAndDecrypt(event.request));
});

self.addEventListener('message', async (event) => {
  const { type } = event.data;
  switch (type) {
    case 'setKey':
      const { key } = event.data;
      // Stored only here as in Serice Worker scope so websites with JavaScript can't access it
      globalThis.key = derive_key(key);
      globalThis.keyFingerprint = await sha256(globalThis.key);
      break;
    case 'setBase':
      globalThis.targetBase = event.data.targetBase;
      console.log("Updated targetBase:", globalThis.targetBase);
      break;
    case 'isKeySet':
      event.source.postMessage({ type, isSet: !!globalThis.key });
      break;
  }
});

function getRealUrl(url) {
  url = new URL(url, globalThis.targetBase);
  if (url.origin === location.origin && url.pathname.startsWith('/swenc-proxy/url')) {
    // It is a cross-origin navigation request with URL embedded
    return new URLSearchParams(url.search).get('url');
  } else if (url.origin === location.origin) {
    // It is a same-origin request, rewrite the origin
    return new URL(url.pathname + url.search + url.hash, globalThis.targetBase).href;
  } else {
    // It is a background cross-origin request
    return url.href;
  }
}
function toFakeUrl(url) {
  url = new URL(url, globalThis.targetBase);
  if (url.origin === location.origin) {
    // If same-origin, we can return relative URL
    return url.pathname + url.search + url.hash;
  } else {
    // Otherwise rewrite so we can intercept it
    const name = url.pathname.split('/').at(-1) || '';
    return new URL(`/swenc-proxy/url/${name}?` + new URLSearchParams({ url }), location.origin).href;
  }
}
function forceHTTPS(url) {
  // Force HTTPS, this disallows some HTTP-only sites but fixes Mixed Content issues
  return new URL(url).href.replace(/^http:/, 'https:');
}

async function fetchThroughProxy(request) {
  const data = {
    url: forceHTTPS(getRealUrl(request.url)),
    method: request.method,
    headers: Array.from(request.headers.entries()),
  }
  if (request.body) {
    data.body = new Uint8Array(await request.arrayBuffer());
  }
  // Some headers aren't seen in 'fetch' yet, so set them manually
  data.headers.push(['sec-fetch-dest', request.destination]);
  data.headers.push(['sec-fetch-mode', request.mode]);
  data.headers.push(['sec-fetch-site', "none"]);
  data.headers.push(['sec-fetch-user', "?1"]);
  data.headers.push(['origin', new URL(globalThis.targetBase).origin]);
  data.headers.push(['referer', globalThis.targetBase]);

  // Set filename for automatic content type detection and download filename
  const filename = new URL(data.url).pathname.split('/').at(-1);

  return {
    response: await fetch(`/swenc-proxy/proxy/${encodeURIComponent(filename)}?` + new URLSearchParams({ key: globalThis.keyFingerprint }), {
      method: 'POST',
      body: serialize_proxy_request(data, globalThis.key),
    }),
    url: data.url,
  };
}

function htmlEncode(str) {
  return str.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
}
function redirectToMain() {
  return new Response(null, {
    status: 302,
    headers: {
      'Location': '/swenc-proxy/',
    },
  });
}

async function fetchAndDecrypt(request) {
  if (!globalThis.key) {
    self.registration.unregister();
    return redirectToMain();
  }

  if (request.mode === "navigate" && (await self.clients.matchAll()).length === 0) {
    // All tabs are closed, reset the origin and back to main page
    delete globalThis.targetBase;
    return redirectToMain();
  }

  const { response, url } = await fetchThroughProxy(request);

  // Create a stream for decrypted content
  const decryptedStream = new ReadableStream({
    async start(controller) {
      if (request.mode === "navigate" &&
        !response.headers.get("Content-Disposition")?.includes("attachment") &&
        response.headers.get("Content-Type")?.includes("text/html")) {

        globalThis.targetBase = url;
        console.log("Updated targetBase from navigation:", globalThis.targetBase);

        // Inject prison.js to intercept navigations and set baseURI for relative URLs
        controller.enqueue(new TextEncoder().encode(`\
<!DOCTYPE html>
<script id="swenc-proxy-prison" src="${htmlEncode(location.origin)}/swenc-proxy/prison.js" data-target-base="${htmlEncode(url)}"></script>
`));
      }

      await decrypt_stream(response.body, controller, globalThis.key)
    },
  });

  let headers = response.headers;
  if (headers.has('X-Location')) {
    // Rewrite Location header because fetch() will follow it
    headers = new Headers({
      ...headers,
      'Location': toFakeUrl(headers.get('X-Location')),
    })
  }
  // Don't include body for status codes that shouldn't have one
  const stream = [101, 204, 205, 304].includes(response.status) ? null : decryptedStream;
  // Return a new Response with the decrypted stream
  return new Response(stream, {
    status: Math.min(response.status, 599), // Limit status code to 599
    statusText: response.statusText,
    headers,
  });
}
