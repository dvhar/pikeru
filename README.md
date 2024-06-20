 # File Picker with Good Thumbnails and Search

Pikeru is a file picker, file searcher, and image viewer for linux that works on any desktop environment or window manager. It has an xdg-desktop-portal backend that makes it useable with web browsers and anything else that uses portal for file selection.

![Screenshot_20240619_212253](https://github.com/dvhar/pikeru/assets/33729230/0029656d-c421-46ce-81f8-bcf9be140bb6)

## Special features other filepickers don't have
* Recursive fuzzy semantic file search can search images by visual content in addition to file name.
* Select multiple directories with ctrl, shift, middle-click, or right-click and click `Open` to view the contents of all of them at the same time.
* Right click an image to view it. Scroll the image to view the next and previous images.
* `Cmd` menu shows commands specified in the config. Click one to run it on the selected files.
* Set a postprocessor script to convert, resize, compress or do anything else with selected files automatically.

## Installation and Usage

First install the dependencies at the bottom of the readme

### Install the filepicker and make applications use it
* Run `./install.sh` to build and install the filepicker and xdg portal.
* To make firefox use the portal, open `about:config` and set `widget.use-xdg-desktop-portal.file-picker` to `1`. Chromium-based browsers should use it by default.

### What if I want my old filepicker back?
* Run `pikeru -d` to disable pikeru and restore your old filepicker, and `pikeru -e` to re-enable pikeru.
* If your xdg-desktop-portal version is older than 1.18, you'll have to delete /usr/share/xdg-desktop-portal/portals/pikeru.portal or remove your $XDG_CURRENT_DESKTOP from that file to disable it.

### How to enable semantic search
* This is configured in `~/.config/xdg-desktop-portal-pikeru/config` in the `indexer` section.
* The config requires 3 things in addition to `enable = true`:
    * a command that prints text associated with a file, like a description or tags
    * a command that checks if the above command can be used, for example checking if an API is online
    * a list of file extensions the above command can handle
* An example configuration using [stable diffusioni webui](https://github.com/AUTOMATIC1111/stable-diffusion-webui)'s `interrogate` API to index your images is included, which uses the `indexer/img_indexer.py` script in this repo. You'll need to edit the filepath and url to use it.
* Pikeru's xdg portal daemon uses the provided command to build a semantic search index of any directory opened or searched by the filepicker so that next time you search that directory, you can search files by semantic content instead of just file name.
* You can pause the indexer with `pikeru -c` and resume it with `pikeru -b`.
* More details are in the man page for xdg-desktop-portal-pikeru.

### What's configured where
The filepicker and the portal that launches the filepicker each have their own config files.
* `~/.config/pikeru.conf`:
    * Anything related to the GUI, like dpi-scale
    * Patterns to ignore when searching
    * Bookmarks saved by drag-and-drop
    * Commands you can run on files
* `~/.config/xdg-desktop-portal-pikeru/config`:
    * Location of filepicker (you probably don't need to touch this part)
    * Postprocessor for selected files
    * Semantic search indexer

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
