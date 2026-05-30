#!/usr/bin/env python3
"""Test client for the embedding server.

Usage:
    python3 test_client.py -t "some text query"
    python3 test_client.py -f /path/to/image.jpg
    python3 test_client.py -t "text" -f /path/to/image.jpg
    python3 test_client.py -t "text1" -T "text2"

When only -t, -T, or -f is provided, prints the first few floats of the embedding.
When two embeddings are provided, prints the dot product of their embeddings.
"""

import argparse
import struct
import sys
import urllib.request
import json
import os

EMBEDDING_DIM = 512
FLOAT_SIZE = 4
EXPECTED_SIZE = EMBEDDING_DIM * FLOAT_SIZE  # 2048 bytes


def fetch_text_embedding(base_url: str, text: str) -> list[float]:
    """POST to /embed/text and return decoded float list."""
    url = f"{base_url.rstrip('/')}/embed/text"
    payload = json.dumps({"text": text}).encode("utf-8")
    req = urllib.request.Request(url, data=payload, headers={"Content-Type": "application/json"})

    with urllib.request.urlopen(req) as resp:
        data = resp.read()

    if len(data) != EXPECTED_SIZE:
        print(f"Warning: expected {EXPECTED_SIZE} bytes, got {len(data)}", file=sys.stderr)

    return list(struct.unpack(f"<{EMBEDDING_DIM}f", data))


def fetch_image_embedding(base_url: str, filepath: str) -> list[float]:
    """POST to /embed/image with multipart/form-data and return decoded float list."""
    url = f"{base_url.rstrip('/')}/embed/image"

    # Read image
    with open(filepath, "rb") as f:
        image_data = f.read()

    # Build multipart form data
    boundary = "----TestBoundary12345"
    filename = os.path.basename(filepath)

    body = (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="file"; filename="{filename}"\r\n'
        f"Content-Type: application/octet-stream\r\n"
        f"\r\n"
    ).encode("utf-8")
    body += image_data
    body += f"\r\n--{boundary}--\r\n".encode("utf-8")

    req = urllib.request.Request(
        url,
        data=body,
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
    )

    with urllib.request.urlopen(req) as resp:
        data = resp.read()

    if len(data) != EXPECTED_SIZE:
        print(f"Warning: expected {EXPECTED_SIZE} bytes, got {len(data)}", file=sys.stderr)

    return list(struct.unpack(f"<{EMBEDDING_DIM}f", data))


def dot_product(a: list[float], b: list[float]) -> float:
    """Compute dot product of two vectors."""
    return sum(x * y for x, y in zip(a, b))


def print_embedding_preview(embedding: list[float], label: str):
    """Print the first few floats of an embedding for inspection."""
    print(f"{label} embedding (first 8 values):")
    for i, v in enumerate(embedding[:8]):
        print(f"  [{i}] {v:.6f}")
    print(f"  ... ({len(embedding)} total)")


def main():
    parser = argparse.ArgumentParser(description="Test client for embedding server")
    parser.add_argument("-t", "--text", type=str, help="Text query for /embed/text")
    parser.add_argument("-T", "--text2", type=str, help="Second text query for comparison")
    parser.add_argument("-f", "--file", type=str, help="Image file path for /embed/image")

    args = parser.parse_args()

    if not args.text and not args.text2 and not args.file:
        parser.error("At least one of -t, -T, or -f is required")

    # Default base URL
    base_url = "http://127.0.0.1:6285"

    emb_a = None
    emb_b = None
    label_a = None
    label_b = None

    def get_text(text: str, label: str) -> list[float] | None:
        try:
            emb = fetch_text_embedding(base_url, text)
        except Exception as e:
            print(f"Error fetching {label} embedding: {e}", file=sys.stderr)
            sys.exit(1)
        return emb

    def get_image(filepath: str, label: str) -> list[float] | None:
        try:
            emb = fetch_image_embedding(base_url, filepath)
        except Exception as e:
            print(f"Error fetching {label} embedding: {e}", file=sys.stderr)
            sys.exit(1)
        return emb

    if args.text:
        emb_a = get_text(args.text, "text")
        label_a = f'"{args.text}"'

    if args.text2:
        emb_b = get_text(args.text2, "text2")
        label_b = f'"{args.text2}"'

    if args.file:
        emb_b = get_image(args.file, "image")
        label_b = args.file

    if emb_a and emb_b:
        dp = dot_product(emb_a, emb_b)
        print(f"Dot product ({label_a} x {label_b}): {dp:.6f}")
    elif emb_a:
        print_embedding_preview(emb_a, label_a)
    elif emb_b:
        print_embedding_preview(emb_b, label_b)


if __name__ == "__main__":
    main()
