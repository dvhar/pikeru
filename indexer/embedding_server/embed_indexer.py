#!/usr/bin/env python3
"""Indexer client for the embedding server.

Uploads an image and writes the embedding to stdout as raw binary:
    2 bytes: u16 LE — number of floats (e.g. 512)
    2 bytes: u16 LE — bit width of each float (e.g. 32)
    N bytes: the embedding vector data

On success, exits 0. On failure, prints to stderr and exits 1.

Usage:
    python3 embed_indexer.py http://127.0.0.1:6285 /path/to/image.jpg
"""

import os
import struct
import sys
import urllib.request
import urllib.error


def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <server_url> <image_path>", file=sys.stderr)
        sys.exit(1)

    base_url = sys.argv[1].rstrip("/")
    image_path = sys.argv[2]

    if not os.path.isfile(image_path):
        print(f"Error: file not found: {image_path}", file=sys.stderr)
        sys.exit(1)

    # Read image data
    with open(image_path, "rb") as f:
        image_data = f.read()

    # Build multipart form data
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
            raw = resp.read()
    except urllib.error.HTTPError as e:
        err_body = e.read().decode("utf-8", errors="replace")
        print(f"Error {e.code}: {err_body}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    # The server returns raw embedding bytes. We prepend a header:
    #   u16 LE: number of floats
    #   u16 LE: bit width of each float (32 for f32)
    # The dim is inferred from the byte count, assuming f32 (4 bytes each).
    # If the response length is not divisible by 4, it's not a valid vector.
    if len(raw) == 0:
        print("Error: empty response from server", file=sys.stderr)
        sys.exit(1)
    if len(raw) % 4 != 0:
        print(
            f"Error: response length {len(raw)} is not a multiple of 4 bytes (expected f32 vector)",
            file=sys.stderr,
        )
        sys.exit(1)

    dim = len(raw) // 4
    bit_width = 32

    # Write header + raw vector to stdout
    sys.stdout.buffer.write(struct.pack("<HH", dim, bit_width))
    sys.stdout.buffer.write(raw)


if __name__ == "__main__":
    main()
