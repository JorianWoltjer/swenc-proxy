import init, { decrypt, derive_key } from './pkg/wasm_dl.js';

console.log('Worker loaded');

self.addEventListener("activate", (event) => {
  event.waitUntil(clients.claim());
});

self.addEventListener('fetch', event => {
  console.log(event);
  const url = new URL(event.request.url);

  // Don't intercept own requests or non-HTTP requests
  if ((url.origin === location.origin && !isDummyRequest(event.request.url)) ||
    (url.protocol !== 'http:' && url.protocol !== 'https:')) {

    event.respondWith(fetch(event.request));
    return;
  }

  console.log("Intercepting", event.request.url);
  event.respondWith(globalThis.fetchAndDecrypt(event.request));
});

self.addEventListener('message', async (event) => {
  await init();  // Always called at the start

  const { type, key } = event.data;
  if (type === 'setKey') {
    globalThis.key = derive_key(key);
  }
});

function isDummyRequest(url) {
  url = new URL(url);
  return url.origin === location.origin && url.pathname === '/dummy';
}
function toDummy(url) {
  return new URL("/dummy?" + new URLSearchParams({ url }), location.origin).href;
}

async function fetchThroughProxy(request) {
  const data = {
    url: request.url,
    method: request.method,
    headers: Array.from(request.headers.entries()),
    // body: btoa(String.fromCharCode(...new Uint8Array(await request.arrayBuffer())))
  }
  if (request.body) {
    data.body = btoa(String.fromCharCode(...new Uint8Array(await request.arrayBuffer())));
  }

  if (isDummyRequest(data.url)) {
    // Get real URL from query string
    const url = new URLSearchParams(new URL(data.url).search).get("url");
    if (!url) {
      throw new Error('No ?url= query parameter in /dummy request');
    }
    data.url = url;
  }

  const filename = new URL(data.url).pathname.split('/').at(-1);
  console.log('Proxying', data);
  return {
    response: await fetch(`/proxy/${filename}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(data),
    }),
    newOrigin: new URL(data.url).origin,
  };
}

async function fetchAndDecrypt(request) {
  if (!globalThis.key) {
    throw new Error('Key not set');
  }

  const { response, newOrigin } = await fetchThroughProxy(request);

  // Create a stream for decrypted content
  const decryptedStream = new ReadableStream({
    async start(controller) {
      if (request.mode == "navigate") {
        controller.enqueue(new TextEncoder().encode(`
<!DOCTYPE html>
<script src="/prison.js"></script>
<base href="${newOrigin.replace(/&/g, '&amp;').replace(/"/g, '&quot;')}">
`));
      }

      await decrypt(response.body, controller, globalThis.key)
    },
  });

  let headers = response.headers;
  if (headers.has('X-Location')) {
    headers = new Headers({
      ...headers,
      'Location': toDummy(headers.get('X-Location')),
    })
  }
  // Return a new Response with the decrypted stream
  const stream = [101, 204, 205, 304].includes(response.status) ? null : decryptedStream;
  return new Response(stream, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

globalThis.fetchAndDecrypt = fetchAndDecrypt;
