#!/usr/bin/env python3
# Embedding indexer for pikeru semantic search.
# Invoked by xdg-desktop-portal-pikeru during indexing to generate CLIP embeddings
# for image files. Outputs raw f32 bytes (little-endian) to stdout — the portal
# reads these as a BLOB and stores them in SQLite.
#
# Usage:
#   python3 embed_indexer.py http://127.0.0.1:6285 /path/to/image.jpg
#
# The server URL must point to the embedding-server HTTP daemon's /embed/image endpoint.

import base64
import json
import struct
import sys
from urllib import request, error


def main():
    if len(sys.argv) < 3:
        print("Usage: embed_indexer.py <server_url> <file_path>", file=sys.stderr)
        quit(1)

    url = sys.argv[1] + "/embed/image"
    file_path = sys.argv[2]

    # Read the image file
    try:
        with open(file_path, "rb") as f:
            img_bytes = f.read()
    except Exception as e:
        print(f"Error reading file: {e}", file=sys.stderr)
        quit(1)

    # Send to embedding server — pass raw bytes as base64 so the server
    # doesn't need to read from disk (avoids path/permission issues).
    ext = file_path.rsplit('.', 1)[-1].lower() if '.' in file_path else "png"
    b64 = base64.b64encode(img_bytes).decode("utf-8")
    data = {"path_b64": b64, "ext": ext}

    headers = {"accept": "application/json", "Content-Type": "application/json"}
    req = request.Request(
        url,
        data=json.dumps(data).encode("utf-8"),
        headers=headers,
        method="POST",
    )

    try:
        with request.urlopen(req) as response:
            resp_text = response.read().decode("utf-8")
            result = json.loads(resp_text)

            if "embedding" not in result:
                print(f"Unexpected response: {resp_text}", file=sys.stderr)
                quit(1)

            emb = result["embedding"]  # list of f32 values from JSON

            # Pack as raw little-endian f32 bytes for the portal to store as BLOB
            sys.stdout.buffer.write(struct.pack(f"{len(emb)}f", *emb))

    except error.HTTPError as e:
        print(f"HTTP error {e.code}: {e.read().decode()}", file=sys.stderr)
        quit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        quit(1)


if __name__ == "__main__":
    main()
