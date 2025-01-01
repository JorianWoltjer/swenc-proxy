#!/usr/bin/env python3
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives.kdf.argon2 import Argon2id
from urllib.parse import urlparse
from getpass import getpass
from hashlib import sha256
import argparse
import requests
import msgpack
import sys
import os

from tqdm import tqdm


SALT = b"swenc-proxy-salt"


def log(*args, **kwargs):
    kwargs.setdefault("file", sys.stderr)
    prompt = kwargs.pop("prompt", "[+]")
    print(prompt, *args, **kwargs)


def normalize_host(url):
    if not url.startswith("http"):
        url = "https://" + url

    url = urlparse(url)
    return url.scheme + "://" + url.netloc


def test_connection(server):
    response = requests.get(server, allow_redirects=False)
    response.raise_for_status()
    return 'swenc-proxy' in response.headers.get("Location")


def derive_key(key):
    kdf = Argon2id(
        salt=SALT,
        length=32,
        memory_cost=19 * 1024,
        iterations=2,
        lanes=1
    )
    return kdf.derive(key.encode())


def get_fingerprint(key):
    return sha256(key).hexdigest()


def encrypt(data, key):
    # Single chunk
    nonce = os.urandom(12)
    cipher = Cipher(algorithms.AES(key), modes.GCM(nonce))
    encryptor = cipher.encryptor()
    ciphertext = encryptor.update(data) + encryptor.finalize() + encryptor.tag
    length = len(ciphertext).to_bytes(4, byteorder="little")
    return nonce + length + ciphertext


def decrypt(r_raw, key):
    # Multiple chunks
    while True:
        nonce = r_raw.read(12)
        if not nonce:
            break
        length = int.from_bytes(r_raw.read(4), byteorder="little")
        ciphertext = r_raw.read(length)
        ciphertext, tag = ciphertext[:-16], ciphertext[-16:]
        cipher = Cipher(algorithms.AES(key), modes.GCM(nonce, tag))
        decryptor = cipher.decryptor()
        yield decryptor.update(ciphertext) + decryptor.finalize()


def serialize_proxy_request(url, key):
    request = {
        "url": url,
        "method": "GET",
        "headers": [("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")],
    }
    serialized = msgpack.packb(request)
    encrypted = encrypt(serialized, key)

    return encrypted


def check_key(key):
    key_fingerprint = get_fingerprint(key)
    log("Using Key:", key_fingerprint)

    response = requests.get(args.server + "/swenc-proxy/check",
                            params={"key": key_fingerprint})
    return response.ok


def fetch_through_proxy(server, url, key, max_redirects=5):
    serialized = serialize_proxy_request(url, key)
    r = requests.post(server + "/swenc-proxy/proxy/",
                      params={"key": get_fingerprint(key)}, data=serialized,
                      allow_redirects=False, stream=True)
    r.raise_for_status()

    if r.status_code // 100 == 3:
        if max_redirects <= 0:
            raise ValueError("Too many redirects")
        yield from fetch_through_proxy(server, r.headers["X-Location"], key, max_redirects - 1)
        return

    content_length = int(r.headers.get('X-Content-Length', 0))
    with tqdm.wrapattr(r.raw, "read", total=content_length) as r_raw:
        r_raw.decode_content = True
        yield from decrypt(r_raw, key)


if __name__ == "__main__":
    args = argparse.ArgumentParser()
    args.add_argument("server", help="SWENC Proxy server to download with")
    args.add_argument("url", help="URL to download")
    args.add_argument("-k", "--key",
                      help="Encryption Key to use (alternatively SWENC_KEY= environment variable or prompted if not provided)")
    args.add_argument("-o", "--output", help="Output file")

    args = args.parse_args()

    args.server = normalize_host(args.server)
    if not test_connection(args.server):
        raise ValueError("Invalid SWENC Proxy Server")
    log("Server is valid")

    if args.key is None:
        if 'SWENC_KEY' in os.environ:
            args.key = os.environ['SWENC_KEY']
        else:
            args.key = getpass("[?] Encryption Key: ")

    args.key = derive_key(args.key)
    if not check_key(args.key):
        raise ValueError("Invalid Key")
    log("Key is valid")

    log("Starting Download...")
    if args.output:
        writer = open(args.output, "wb")
    else:
        writer = sys.stdout.buffer

    for chunk in fetch_through_proxy(args.server, args.url, args.key):
        writer.write(chunk)

    writer.flush()
    log(f"Wrote to {args.output or 'stdout'}")
    writer.close()
