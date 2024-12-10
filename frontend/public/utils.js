import init from "/swenc-proxy/pkg/frontend.js";
export { decrypt_stream, derive_key, serialize_proxy_request } from "/swenc-proxy/pkg/frontend.js";

init();

export async function sha256(buf) {
    let hash = await crypto.subtle.digest("SHA-256", buf);
    hash = Array.from(new Uint8Array(hash));
    return hash
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");
}
