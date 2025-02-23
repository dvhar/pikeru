## Semantic Search Caption Generator Server

This generates the searchable text for the file picker. It is not installed by
the main install script because you may want to run it on a dedicated server.

### How to install it

This installs it in /opt, creates a systemd service for it called
`caption-server`, and runs it. It uses CPU by default, but you can enable GPU
if you don't mind the extra vram usage by removing `-d cpu` from
/opt/caption_server/start.sh.

```
sudo ./install.sh
```

### How to use it

This command is invoked by xdg-desktop-portal-pikeru and defined in its config
file. The endpoint can be /caption or /sdapi/v1/interogate, for compatibility
with stable-diffusion-webui's api. This server was made so you don't have to
deal with unstable apis like that.

```
python /path/to/img_indexer.py http://<your server>:7860/caption /path/to/image.jpg
```
