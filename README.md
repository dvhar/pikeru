 # File Picker with Good Thumbnails and Search

Pikeru is a file picker, file searcher, and image viewer for linux that works on any desktop environment or window manager. It has an xdg-desktop-portal backend that makes it useable with web browsers and anything else that uses portal for file selection.

![Screenshot_20240321_072233_resized](https://github.com/dvhar/pikeru/assets/33729230/6257fa5e-e94e-4d3e-8dad-b4269e2d1ad3)

## Special features other filepickers don't have
* Recursive fuzzy semantic file search can search images by visual content in addition to file name.
* Select multiple directories with ctrl, shift, middle-click, or right-click and click `Open` to view the contents of all of them at the same time.
* Right click an image to view it. Scroll the image to view the next and previous images.
* `Cmd` menu shows commands specified in `~/.config/pikeru.conf`. Click one to run it on the selected files.
* Set a postprocessor script in the config file to convert, resize, compress or do anything else with selected files automatically.

## Installation and Usage

First install the dependencies at the bottom of the readme

### Install the filepicker and make applications use it
* Run `./install.sh` to build and install the filepicker and xdg portal.
* To make firefox use the portal, open `about:config` and set `widget.use-xdg-desktop-portal.file-picker` to `1`.
* If your chromium-based browser is not using xdg portal for whatever reason, you can still use pikeru by setting environment variable `XDG_CURRENT_DESKTOP=KDE` and putting the `kdialog` script in your path to trick the browser into thinking pikeru is kdialog.

### What if I want my old filepicker back?
* pikeru's `-e` and `-d` flags configure xdg portal to enable and disable pikeru so it's easy to switch back to your old filepicker. Those flags are just wrappers for `setconfig.sh` and `unsetconfig.sh`.

### How to enable semantic search
* This is configured in `~/.config/xdg-desktop-portal-pikeru/config` in the `indexer` section.
* The config requires 3 things in addition to `enable = true`:
    * a command that prints text associated with a file, like a description or tags
    * a command that checks if the above command can be used, for example checking if an API is online
    * a list of file extensions the above command can handle
* An example configuration using [stable diffusioni webui](https://github.com/AUTOMATIC1111/stable-diffusion-webui)'s `interrogate` API to index your images is included, which uses the `indexer/img_indexer.py` script in this repo. You'll need to edit the filepath and url to use it.
* Pikeru's xdg portal daemon uses the provided command to build a semantic search index of any directory opened or searched by the filepicker so that next time you search that directory, you can search files by semantic content instead of just file name.

## Install Dependencies

### Ubuntu:
```
sudo apt install build-essential scdoc pkg-config libavutil-dev libavformat-dev libavfilter-dev libavdevice-dev libclang-dev
```

### Arch:
```
sudo pacman -S scdoc xdg-desktop-portal ffmpeg clang
```

## License
Pikeru is Public Domain.
