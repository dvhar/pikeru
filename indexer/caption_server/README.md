## Semantic Search Caption Generator Server

This generates the searchable text for the file picker. It is not installed by the install script because you may want to run it on a dedicated server with a GPU.

### How to run it

```
python -m pip venv venv
. ./venv/bin/activate
pip install -r requirements.txt
python ./run_api.py
```

### How to use it

This command is invoked by xdg-desktop-portal-pikeru. The endpoint can be
/caption or /sdapi/v1/interogate, for compatibility with
stable-diffusion-webui's api. This server was made so you don't have to deal
with unstable apis like that.

```
python /path/to/img_indexer.py http://<your server>:7860/caption /path/to/image.jpg
```
