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

### CLI options

```
Usage: embedding-server [OPTIONS]
  --port PORT       Port to listen on (default: 6285)
  --cache-dir DIR   Directory to store downloaded models
                   (default: $FASTEMBED_CACHE_DIR or .fastembed_cache)
```

Models are downloaded from HuggingFace on first run and cached in `--cache-dir`
(or `$FASTEMBED_CACHE_DIR` / `$HF_HOME` if set). Subsequent starts use the cached
models without network access.

### HTTP endpoints

| Method | Endpoint       | Body                     | Response                            |
|--------|----------------|--------------------------|-------------------------------------|
| GET    | `/health`      | —                        | `{"status": "ok"}`                  |
| POST   | `/embed/image` | multipart/form-data with image binary and filename | binary response (512-len f32 array)
| POST   | `/embed/text`  | `{"text": "query"}`      | binary response (512-len f32 array)

### CLI client — embed_indexer.py

```bash
# Image indexing
python3 embed_indexer.py http://127.0.0.1:6285 /path/to/image.jpg

# Text search query
python3 embed_indexer.py http://127.0.0.1:6285 "query:a cat sitting on a wall"
```

Writes a 4-byte header (u16 dim + u16 float bit width, little-endian) followed by
the raw embedding vector bytes to stdout. On TTY, prints a human-readable preview
instead. Exits 1 with message to stderr on failure.

### Test client — test_client.py

```
python3 test_client.py -t "some text query"
python3 test_client.py -f /path/to/image.jpg
python3 test_client.py -t "text1" -T "text2"
python3 test_client.py -t "text" -f /path/to/image.jpg
```

- `-t <text>`: hits /embed/text with the given text
- `-T <text>`: second text query for comparison
- `-f <filepath>`: hits /embed/image with the given image data

With one flag, prints the first 8 floats of the embedding. With two flags, prints
the dot product of their response vectors.
