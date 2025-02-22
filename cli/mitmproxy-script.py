"""Send a reply from the proxy without sending the request to the remote server."""

import logging
from mitmproxy import http
from io import BytesIO
import requests
import sys
import os

from download import decrypt, derive_key, check_key, encrypt, get_fingerprint, serialize_proxy_request


def get_env(name: str) -> str:
    try:
        return os.environ[name]
    except KeyError:
        print(f"Please set the {name} environment variable.", file=sys.stderr)
        raise


KEY = get_env("SWENC_KEY")
SERVER = get_env("SWENC_SERVER")

KEY = derive_key(KEY)
if not check_key(SERVER, KEY):
    raise ValueError(
        f"Invalid SWENC_KEY ({get_fingerprint(KEY)}), not recognized by {SERVER}")


def request(flow: http.HTTPFlow) -> None:
    headers = list(flow.request.headers.items(multi=True))
    serialized = serialize_proxy_request(
        flow.request.url, flow.request.method, headers, flow.request.content)
    encrypted = encrypt(serialized, KEY)

    r = requests.post(SERVER + "/swenc-proxy/proxy/",
                      params={"key": get_fingerprint(KEY)}, data=encrypted,
                      allow_redirects=False)

    # mitmproxy cannot stream responses (https://github.com/mitmproxy/mitmproxy/discussions/5277), so for now we'll have to live with sending it in one go
    body = b"".join(decrypt(BytesIO(r.content), KEY))
    logging.info(f"Received {body[:100]} and {r.headers}")

    for header in ["Location", "Content-Length", "Content-Encoding"]:
        if r.headers.get(f"X-{header}"):
            r.headers[header] = r.headers.pop(f"X-{header}")
        else:
            r.headers.pop(header, None)
    r_headers = [(k.encode(), v.encode()) for k, v in r.headers.items()]

    flow.response = http.Response.make(
        r.status_code,
        b"",
        r_headers,
    )
    flow.response.raw_content = body
