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
| POST   | `/embed/image` | multipart/form-data with image binary and filename | binary response (512-len f32 array)
| POST   | `/embed/text`  | `{"text": "query"}`      | binary response (512-len f32 array)

### CLI client — embed_indexer.py

```bash
python3 embed_indexer.py http://127.0.0.1:6285 /path/to/image.jpg
```

Prints raw f32 bytes (little-endian, 512 × 4 = 2048 bytes) to stdout for the
portal's indexing pipeline. Mirrors the `img_indexer.py` interface exactly.

### Test client - test_client.python3

Take 2 flags, only one is needed both both can be used.
-t <text>: hits the /embed/text endpoint with the given text arguemnt
-f <filepath>: hits the /embed/image endpoint with the given image data
Each will print the first few floats of the embedding vector.
If both are used, it will print the dot product of their response vectors.
