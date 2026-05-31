#!/usr/bin/env python3
"""Indexer client for the embedding server.

Uploads an image or sends a text query and writes the embedding to stdout
as raw binary:
    2 bytes: u16 LE — number of floats (e.g. 512)
    2 bytes: u16 LE — bit width of each float (e.g. 32)
    N bytes: the embedding vector data

On success, exits 0. On failure, prints to stderr and exits 1.

Usage:
    # Image indexing
    python3 vector_indexer.py http://127.0.0.1:6285 "index:/path/to/image.jpg"

    # Text search query
    python3 vector_indexer.py http://127.0.0.1:6285 "query:a cat on a wall"
"""

import json
import os
import struct
import sys
import urllib.request
import urllib.error


def write_output(raw: bytes) -> None:
    """Validate response and write header + vector to stdout."""
    if len(raw) == 0:
        print("Error: empty response from server", file=sys.stderr)
        sys.exit(1)
    if len(raw) % 4 != 0:
        print(
            f"Error: response length {len(raw)} is not a multiple of 4 bytes "
            f"(expected f32 vector)",
            file=sys.stderr,
        )
        sys.exit(1)

    dim = len(raw) // 4
    bit_width = 32

    # When stdout is a TTY, print a human-readable summary instead of binary.
    if sys.stdout.isatty():
        values = struct.unpack(f"<{dim}f", raw)
        preview = ", ".join(f"{v:.5f}" for v in values[:8])
        print(f"len:{dim} floatsize:{bit_width} [{preview}, ...]")
    else:
        sys.stdout.buffer.write(struct.pack("<HH", dim, bit_width))
        sys.stdout.buffer.write(raw)


def fetch_image_embedding(base_url: str, image_path: str) -> bytes:
    """POST image to /embed/image and return raw response body."""
    if not os.path.isfile(image_path):
        print(f"Error: file not found: {image_path}", file=sys.stderr)
        sys.exit(1)

    with open(image_path, "rb") as f:
        image_data = f.read()

    boundary = "----EmbedBoundary"
    filename = os.path.basename(image_path)

    body = (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="file"; filename="{filename}"\r\n'
        f"Content-Type: application/octet-stream\r\n"
        f"\r\n"
    ).encode("utf-8")
    body += image_data
    body += f"\r\n--{boundary}--\r\n".encode("utf-8")

    url = f"{base_url}/embed/image"
    req = urllib.request.Request(
        url,
        data=body,
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
    )

    try:
        with urllib.request.urlopen(req) as resp:
            return resp.read()
    except urllib.error.HTTPError as e:
        err_body = e.read().decode("utf-8", errors="replace")
        print(f"Error {e.code}: {err_body}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


def fetch_text_embedding(base_url: str, text: str) -> bytes:
    """POST text to /embed/text and return raw response body."""
    if not text:
        print("Error: empty query text", file=sys.stderr)
        sys.exit(1)

    url = f"{base_url}/embed/text"
    payload = json.dumps({"text": text}).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=payload,
        headers={"Content-Type": "application/json"},
    )

    try:
        with urllib.request.urlopen(req) as resp:
            return resp.read()
    except urllib.error.HTTPError as e:
        err_body = e.read().decode("utf-8", errors="replace")
        print(f"Error {e.code}: {err_body}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


QUERY_PREFIX = "query:"
INDEX_PREFIX = "index:"


def main():
    if len(sys.argv) != 3:
        print(
            f"Usage: {sys.argv[0]} <server_url> <index:path | query:text>",
            file=sys.stderr,
        )
        sys.exit(1)

    base_url = sys.argv[1].rstrip("/")
    arg = sys.argv[2]

    if arg.startswith(QUERY_PREFIX):
        text = arg[len(QUERY_PREFIX):]
        raw = fetch_text_embedding(base_url, text)
    elif arg.startswith(INDEX_PREFIX):
        image_path = arg[len(INDEX_PREFIX):]
        raw = fetch_image_embedding(base_url, image_path)
    else:
        print(
            f"Error: argument must start with '{QUERY_PREFIX}' or '{INDEX_PREFIX}'",
            file=sys.stderr,
        )
        sys.exit(1)

    write_output(raw)


if __name__ == "__main__":
    main()
