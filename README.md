 # File Picker with Good Thumbnails and Search

Pikeru is a file picker, file searcher, and image viewer for linux that works on any desktop environment or window manager. It has an xdg-desktop-portal backend that makes it useable with web browsers and anything else that uses portal for file selection.

![Screenshot_20240821_205129](https://github.com/user-attachments/assets/c24d034a-e3bd-4199-a251-6c9b5d4e4794)


## Special features other filepickers don't have
* Search images by semantic content in addition to file name.
* Select multiple directories and click `Open` to view the contents of all of them at the same time.
* Right click an image to view it. Scroll the image to view the next and previous images.
* Command menu shows commands specified in the config. Click one to run it on the selected files.
* Set a postprocessor script to convert or do anything else with selected files automatically.

## Installation and Usage

### Install the filepicker and make applications use it
* On arch, you can install the `pikeru` AUR package and then run `pikeru -e` to enable it.
* Otherwise, install the dependencies at the bottom of the readme and Run `./install.sh`.
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
* A caption generator is included in `indexer/caption_server` which can run on this computer or separate server to generate searchable text for images.
* The example configuration uses `indexer/img_indexer.py` to communicate with the caption generator and build the search index.
* Pikeru's xdg portal daemon uses the provided command to build a semantic search index of any directory opened or searched by the filepicker so that next time you search that directory, you can search files by semantic content instead of just file name.
* You can clear the indexer queue with `pikeru -c` if you don't want it to index the current batch.
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

### How do I make pikeru a floating window when using a tiling window manager?
* Pikeru should handle this automatically, but if not, set `resizeable` to `false` or `sometimes` in ~/.config/pikeru.conf.
* Alternatively, configure your window manager to give pikeru a floating window by setting rules based on the window title. The first option will make it a floating window, but on some systems also makes it unresizable, so this option may be preferable. This is more likely to be an issue on X11.

## Install Dependencies

### Ubuntu:
```
sudo apt install build-essential scdoc pkg-config libavutil-dev libavformat-dev libavfilter-dev libavdevice-dev libclang-dev
```

### Arch:
```
sudo pacman -S scdoc xdg-desktop-portal ffmpeg clang
```

#### Optional:
To enable pdf and epub thumbnails, make sure `pdftoppm` and `epub-thumbnailer` are installed.

## License
MIT License
