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
