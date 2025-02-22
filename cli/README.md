# CLI

This directory contains a [download.py](download.py) Python script that can interact with a swenc-proxy server to download a single URL. This is useful for environments where a browser is not available, but you still want to download a blocked file through the command-line.

By default, it outputs the file to STDOUT, but with the `-o` argument it can be written to any location directly.  

A few packages are required for this tool, which can be installed like this:

```sh
python -m pip install -r requirements.txt
```

> [!TIP]
> If the target machine where you want to run this tool on does not have `python` or `pip` installed, and you cannot easily install it, consider making a binary out of it using [PyInstaller](https://pyinstaller.org/en/stable/).

## Usage

```shell
$ python download.py --help

usage: download.py [-h] [-k KEY] [-o OUTPUT] server url

positional arguments:
  server                SWENC Proxy server to download with
  url                   URL to download

options:
  -h, --help            show this help message and exit
  -k KEY, --key KEY     Encryption Key to use (alternatively SWENC_KEY= environment variable or prompted if not provided)
  -o OUTPUT, --output OUTPUT
                        Output file
```

## Examples

```shell
$ ./download.py http://localhost:8000 'https://example.com' -o example.html

[+] Server is valid
[?] Encryption Key: secret
[+] Using Key: 1b505cb50954d5e5aba33da6a2e6ee19e18745a7eb9923679005b98e4b10acae
[+] Key is valid
[+] Starting Download...
1.26kB [00:00, 1.27MB/s]     
[+] Wrote to example.html
```

## mitmproxy

Another script named [mitmproxy-script.py](mitmproxy-script.py) is an addon for [mitmproxy](https://mitmproxy.org/), a terminal-based proxy server. This allows you to globally configure a proxy and send all HTTP traffic of commands through swenc-proxy by combining it with [proxychains](https://mitmproxy.org/).

### Setup

First, [install mitmproxy](https://docs.mitmproxy.org/stable/overview-installation/) and the dependencies of the addon using the following commands:

```sh
pipx install mitmproxy
pipx inject mitmproxy requests 'cryptography>=44.0.0' msgpack tqdm
```

Set up [proxychains](https://github.com/haad/proxychains) (`apt install proxychains4`) to easily connect any tool to mitmproxy. It should be configured in `/etc/proxychains4.conf` with the following text at the bottom:

```toml
[ProxyList]
# socks4 127.0.0.1 9050
http 127.0.0.1 1080
```

Then, start `mitmproxy` on port 1080 and get the CA certificate:

```sh
mitmproxy -p 1080
proxychains curl 'http://mitm.it/cert/pem' -o mitmproxy.crt
```

Verify that it is a valid PEM certificate with a `-----BEGIN CERTIFICATE-----` header, and install it. Below are [instructions for Ubuntu](https://ubuntu.com/server/docs/install-a-root-ca-certificate-in-the-trust-store):

```sh
sudo apt-get install -y ca-certificates
sudo cp mitmcert.crt /usr/local/share/ca-certificates
sudo update-ca-certificates
```

### Usage

Now that this configuration is done, you are ready to use [mitmproxy-script.py](mitmproxy-script.py) as an addon to `mitmproxy` with the `-s` argument. This will send all requests to swenc-proxy and decrypt the response. You must configure the swenc server and key you want to use with the `SWENC_SERVER` and `SWENC_KEY` environment variables, like so:

```sh
export SWENC_SERVER=http://localhost:8000
export SWENC_KEY=secret
mitmproxy -p 1080 -s swenc-proxy/cli/mitmproxy-script.py
```

With this active you are ready to use `proxychains` in front of any command to redirect its HTTP requests through swenc-proxy. With environment variables like `HTTP_PROXY` and `HTTPS_PROXY` set to `http://localhost:1080` you can sometimes do the same, as well as custom configuration in other applications.

```shell
$ proxychains curl https://example.com
[proxychains] config file found: /etc/proxychains4.conf
[proxychains] preloading /usr/lib/x86_64-linux-gnu/libproxychains.so.4
[proxychains] DLL init: proxychains-ng 4.16
[proxychains] Strict chain  ...  127.0.0.1:1080  ...  example.com:443  ...  OK
<!doctype html>
<html>
<head>
    <title>Example Domain</title>
    ...
```

> [!TIP]  
> For commands requiring `sudo`, put it *in front* of proxychains like `sudo proxychains apt upgrade` to allow it to intercept the network calls. Otherwise the proxy is ignored.
