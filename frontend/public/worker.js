import init, { decrypt_stream, derive_key, serialize_proxy_request } from '/swenc-proxy/pkg/frontend.js';

console.log('Worker loaded');

self.addEventListener("activate", (event) => {
  event.waitUntil(clients.claim());
});

function isMetaRequest(url) {
  // Any requests to /swenc-proxy shouldn't be intercepted, except for /url for dummy requests
  console.log(url);
  return url.origin === location.origin && url.pathname.startsWith('/swenc-proxy') &&
    url.pathname !== '/swenc-proxy/url';
}

self.addEventListener('fetch', async (event) => {
  console.log(event);
  const url = new URL(event.request.url);

  // Don't intercept meta or non-HTTP requests
  if (isMetaRequest(url) ||
    (url.protocol !== 'http:' && url.protocol !== 'https:')) {

    event.respondWith(fetch(event.request));
    return;
  }

  // Start proxying
  console.log("Intercepting", event.request.url);
  event.respondWith(fetchAndDecrypt(event.request));
});

self.addEventListener('message', async (event) => {
  // Always called at the start
  await init();

  console.log('Message', event);

  const { type, key } = event.data;
  switch (type) {
    case 'setKey':
      // Stored only here as in Serice Worker scope so websites with JavaScript can't access it
      globalThis.key = derive_key(key);
      break;
    case 'setTargetOrigin':
      globalThis.targetOrigin = event.data.origin;
      break;
  }
});

// TODO: generalize these functions to 1 module. Maybe a rewriter class with .from() and .to()
function getRealUrl(url) {
  // Based on .href here to include relative directory
  url = new URL(url, location.href);
  if (url.origin === location.origin && url.pathname === '/swenc-proxy/url') {
    // It is a cross-origin navigation request with URL embedded
    return new URLSearchParams(url.search).get('url');
  } else if (url.origin === location.origin) {
    // It is a same-origin request, rewrite the origin
    return new URL(url.pathname + url.search + url.hash, globalThis.targetOrigin || location.origin).href;
  } else {
    // It is a background cross-origin request
    return url.href;
  }
}
function toFakeUrl(url) {
  if (url.origin === location.origin) {
    // If same-origin, we can return relative URL
    return url.pathname + url.search + url.hash;
  } else {
    // Otherwise rewrite so we can intercept it
    return new URL("/swenc-proxy/url?" + new URLSearchParams({ url }), location.origin).href;
  }
}
function forceHTTPS(url) {
  // Force HTTPS, this disallows some HTTP-only sites but fixes Mixed Content issues
  return new URL(url).href.replace(/^http:/, 'https:');
}

async function fetchThroughProxy(request) {
  console.log('Request', request);
  const data = {
    url: forceHTTPS(getRealUrl(request.url)),
    method: request.method,
    headers: Array.from(request.headers.entries()),
  }
  console.log('Request Data', data);
  if (request.body) {
    data.body = btoa(String.fromCharCode(...new Uint8Array(await request.arrayBuffer())));
  }

  const filename = new URL(data.url).pathname.split('/').at(-1);
  let newOrigin = new URL(data.url).origin;

  console.log('Proxying', data);
  return {
    response: await fetch(`/swenc-proxy/proxy/${filename}`, {
      method: 'POST',
      body: serialize_proxy_request(data, globalThis.key),
    }),
    newOrigin,
  };
}

function htmlEncode(str) {
  return str.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
}

async function fetchAndDecrypt(request) {
  if (!globalThis.key) {
    throw new Error('Key not set');
  }

  if (request.mode == "navigate") {
    if ((await self.clients.matchAll()).length === 0) {
      // All tabs are closed, reset the origin and back to main page
      console.log("All tabs closed, resetting target origin");
      globalThis.targetOrigin = null;
      return new Response(null, {
        status: 302,
        headers: {
          'Location': '/swenc-proxy/',
        },
      });
    }
  }

  const { response, newOrigin } = await fetchThroughProxy(request);

  // Create a stream for decrypted content
  const decryptedStream = new ReadableStream({
    async start(controller) {
      if (request.mode == "navigate") {
        globalThis.targetOrigin = newOrigin;

        console.log("Navigation request, injecting prison.js");
        // Inject prison.js to intercept navigations and set baseURI for relative URLs
        controller.enqueue(new TextEncoder().encode(`
<!DOCTYPE html>
<script id="swenc-proxy-prison" src="${htmlEncode(location.origin)}/swenc-proxy/prison.js" data-swenc-proxy-origin="${htmlEncode(newOrigin)}"></script>
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
      'Location': toFakeUrl(new URL(headers.get('X-Location'), location.href).href),
    })
  }
  // Don't include body for status codes that shouldn't have one
  const stream = [101, 204, 205, 304].includes(response.status) ? null : decryptedStream;
  // Return a new Response with the decrypted stream
  console.log("Returning stream");
  return new Response(stream, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}
