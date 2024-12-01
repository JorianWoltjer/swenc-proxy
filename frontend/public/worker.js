import init, { decrypt, derive_key } from './pkg/wasm_dl.js';

console.log('Worker loaded');

self.addEventListener("activate", (event) => {
  event.waitUntil(clients.claim());
});

self.addEventListener('fetch', event => {
  const pathToIntercept = '/proxy';

  const url = new URL(event.request.url);

  // Intercept only the desired URL
  if (url.pathname === pathToIntercept) {
    event.respondWith(globalThis.fetchAndDecrypt(event.request));
  }
});

self.addEventListener('message', async (event) => {
  await init();  // Always called at the start

  const { type, key } = event.data;
  if (type === 'setKey') {
    globalThis.key = derive_key(key);
  }
});

(async () => {
  async function fetchAndDecrypt(request) {
    if (!globalThis.key) {
      throw new Error('Key not set');
    }

    const response = await fetch(request);

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    // Create a stream for decrypted content
    // TODO: if this is impossible, maybe external progress bar
    const decryptedStream = new ReadableStream({
      async start(controller) {
        await decrypt(response.body, controller, globalThis.key)
      },
    });

    // Return a new Response with the decrypted stream
    return new Response(decryptedStream, {
      headers: { 'Content-Type': 'application/octet-stream' },
    });
  }

  globalThis.fetchAndDecrypt = fetchAndDecrypt;
})();
