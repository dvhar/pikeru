#!/usr/bin/env python
# Example indexer works with caption_server or with stable-diffusion-webui to
# generate searchable text for images. Not guaranteed to be compatible with the
# latest version of stable-diffusion-webui, but it does work with the provided
# caption server. It also works with the provided embedding server.
#
# This is invoked by xdg-desktop-portal-pikeru to build a semantic search index,
# see usage info in its config file.
#
# Operating modes (controlled by environment variables):
#   PK_INDEX_EMBEDDING=1
#       Upload the file to <base_url>/embed/image and write the raw binary
#       embedding (4-byte header + f32 vector) to stdout. Used with
#       xdg-desktop-portal-pikeru's indexer mode=vector.
#   PK_QUERY_EMBEDDING=1
#       Send the query text to <base_url>/embed/text and write the raw binary
#       embedding to stdout. Used for text search queries.
#   (neither set) Default: POST the image to the caption server at the given URL and
#       print the caption text.
import base64, json, os, struct, sys
from urllib import request, error
from urllib.parse import urlparse, urlunsplit

if len(sys.argv) < 3:
    quit(1)

# For vector/query embedding modes we only need scheme+host (the /embed/<subpath>
# is appended by the mode functions). For plain text mode we need the full
# URL as-is (e.g. http://localhost:7860/sdapi/v1/interrogate).
raw_base = sys.argv[1].rstrip("/")
parsed = urlparse(raw_base)
base_url = urlunsplit((parsed.scheme, parsed.netloc, "", "", ""))
full_url = raw_base  # keep original for captioning mode

arg = sys.argv[2]

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def write_embedding(raw: bytes) -> None:
    """Validate response and write header + vector to stdout.

    When stdout is a TTY, print a human-readable summary instead of binary.
    """
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

    if sys.stdout.isatty():
        dim = len(raw) // 4
        bit_width = 32
        values = struct.unpack(f"<{dim}f", raw)
        preview = ", ".join(f"{v:.5f}" for v in values[:8])
        print(f"len:{dim} floatsize:{bit_width} [{preview}, ...]")
    else:
        dim = len(raw) // 4
        bit_width = 32
        sys.stdout.buffer.write(struct.pack("<HH", dim, bit_width))
        sys.stdout.buffer.write(raw)


# ---------------------------------------------------------------------------
# Embedding modes
# ---------------------------------------------------------------------------

def embed_image(base_url: str, image_path: str) -> None:
    """POST image to /embed/image and write raw binary embedding to stdout."""
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
    req = request.Request(
        url,
        data=body,
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
    )

    try:
        with request.urlopen(req) as resp:
            write_embedding(resp.read())
    except error.HTTPError as e:
        print(f"Error {e.code}: {e.read().decode('utf-8', errors='replace')}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


def embed_query(base_url: str, query_text: str) -> None:
    """POST text to /embed/text and write raw binary embedding to stdout."""
    url = f"{base_url}/embed/text"
    payload = json.dumps({"text": query_text}).encode("utf-8")
    req = request.Request(
        url,
        data=payload,
        headers={"Content-Type": "application/json"},
    )

    try:
        with request.urlopen(req) as resp:
            write_embedding(resp.read())
    except error.HTTPError as e:
        print(f"Error {e.code}: {e.read().decode('utf-8', errors='replace')}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


# ---------------------------------------------------------------------------
# Dispatch based on environment variables
# ---------------------------------------------------------------------------

if os.environ.get("PK_INDEX_EMBEDDING"):
    embed_image(base_url.rstrip("/"), arg)
elif os.environ.get("PK_QUERY_EMBEDDING"):
    embed_query(base_url.rstrip("/"), arg)
else:
    # ---------------------------------------------------------------------------
    # Default mode — text captioning
    # ---------------------------------------------------------------------------
    with open(arg, "rb") as image_file:
        img = base64.b64encode(image_file.read()).decode('utf-8')
    headers = {'accept': 'application/json', 'Content-Type': 'application/json'}
    data = {'image': img, 'model': 'clip'}
    req = request.Request(
        full_url,
        data=json.dumps(data).encode('utf-8'),
        headers=headers,
        method='POST'
    )
    try:
        with request.urlopen(req) as response:
            resp_text = response.read().decode('utf-8')
            response_dict = json.loads(resp_text)
            print(response_dict.get('caption', ''))
    except error.HTTPError as e:
        print(e.read().decode(), file=sys.stderr)
        quit(1)
    except Exception as e:
        print(str(e), file=sys.stderr)
        quit(1)
