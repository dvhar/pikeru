## Embedding Server for Pikeru Semantic Search

This is a Rust-based HTTP daemon that generates CLIP embeddings, serving as the
replacement for `caption_server`. Instead of producing human-readable captions, it
outputs raw embedding vectors — enabling semantic search in pikeru.

### How it works

- **Image encoding**: Uses fastembed's CLIP vision encoder (`Qdrant/clip-ViT-B-32-vision`)
  to turn images into 512-dim vectors.
- **Text encoding**: Uses fastembed's CLIP text encoder (`ClipVitB32`) to turn user
  queries into the same 512-dim space.
- Both models are loaded once at startup and reused for every request.

### HTTP endpoints

| Method | Endpoint       | Body                     | Response                            |
|--------|----------------|--------------------------|-------------------------------------|
| GET    | `/health`      | —                        | `{"status": "ok"}`                  |
| POST   | `/embed/image` | `{"path_b64": "..."}`    | `{"dim": 512, "embedding": [...]}` |
| POST   | `/embed/text`  | `{"text": "query"}`      | `{"dim": 512, "embedding": [...]}` |

The server accepts images either as a file path (`path`) or base64-encoded bytes
(`path_b64`). The Python `embed_indexer.py` script sends base64.

### CLI client — embed_indexer.py

```bash
python3 embed_indexer.py http://127.0.0.1:6285 /path/to/image.jpg
```

Prints raw f32 bytes (little-endian, 512 × 4 = 2048 bytes) to stdout for the
portal's indexing pipeline. Mirrors the `img_indexer.py` interface exactly.

### How to install it

```bash
sudo ./install.sh
```

This builds the Rust project, installs the binary to `/opt/embedding_server`, and
creates a systemd service called `embedding-server`. The first run downloads CLIP
models from HuggingFace (~250MB total).

### How to use it with pikeru

Update your portal config (`~/.config/xdg-desktop-portal-pikeru/config`):

```ini
[indexer]
enable = true

cmd = python3 /opt/embedding_server/embed_indexer.py http://127.0.0.1:6285
check = curl -f http://127.0.0.1:6285/health

extensions = png,jpg,jpeg,gif,webp,tiff,bmp
```

### Requirements

- Rust toolchain (rustc + cargo)
- Python 3 (for `embed_indexer.py`)
- Internet access on first run (downloads CLIP models from HuggingFace Hub)
