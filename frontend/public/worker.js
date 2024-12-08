import init, { decrypt, derive_key } from '/swenc-proxy/pkg/wasm_dl.js';

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
      globalThis.key = derive_key(key);
      break;
    case 'setTargetOrigin':
      globalThis.targetOrigin = event.data.origin;
      break;
  }
});

// TODO: generalize these functions to 1 module. Maybe a rewriter class with .from() and .to()
function getRealUrl(url) {
  url = new URL(url, location.origin);
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
  if (url.origin === location.origin || url.origin === globalThis.targetOrigin) {
    // If same-origin, we can return relative URL
    return url.pathname + url.search + url.hash;
  } else {
    // Otherwise rewrite so we can intercept it
    return new URL("/swenc-proxy/url?" + new URLSearchParams({ url }), location.origin).href;
  }
}

async function fetchThroughProxy(request) {
  console.log('Request', request);
  const data = {
    url: getRealUrl(request.url),
    method: request.method,
    headers: Array.from(request.headers.entries()),
  }
  console.log('Request Data', data);
  if (request.body) {
    data.body = btoa(String.fromCharCode(...new Uint8Array(await request.arrayBuffer())));
  }

  const filename = new URL(data.url).pathname.split('/').at(-1);
  let newOrigin = new URL(data.url).origin;
  // Our proxy origin should be target origin
  newOrigin = newOrigin === location.origin ? globalThis.targetOrigin : newOrigin;

  console.log('Proxying', data);
  return {
    response: await fetch(`/swenc-proxy/proxy/${filename}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(data),
    }),
    newOrigin,
  };
}

function htmlEncode(str) {
  return str.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
}

async function fetchAndDecrypt(request) {
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
        console.log("Navigation request, injecting prison.js");
        // Inject prison.js to intercept navigations and set baseURI for relative URLs
        controller.enqueue(new TextEncoder().encode(`
<!DOCTYPE html>
<base href="${htmlEncode(newOrigin)}">
<script src="${htmlEncode(location.origin)}/swenc-proxy/prison.js"></script>
`));
      }

      await decrypt(response.body, controller, globalThis.key)
    },
  });

  let headers = response.headers;
  if (headers.has('X-Location')) {
    // Rewrite Location header because fetch() will follow it
    headers = new Headers({
      ...headers,
      'Location': toFakeUrl(new URL(headers.get('X-Location'), newOrigin).href),
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
