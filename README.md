 # Pikeru: The File Picker with Good Thumbnails

Pikeru is a filepicker for linux that has working thumbnails and works on any desktop environment or window manager. Kind of like kdialog but better and with some added features.

![Screenshot_20240321_072233_resized](https://github.com/dvhar/pikeru/assets/33729230/6257fa5e-e94e-4d3e-8dad-b4269e2d1ad3)

## Installation and Usage

### Install the filepicker
* Install `tk` with your system package manager. Or whichever tkinter-related package python complains about when you run it.
* Make sure the python `venv` module is installed.
* Run `./pikeru`. That will create the venv and config file the first time and launch the filepicker.
* If using a high-dpi display, edit `dpi_scale` in ~/.config/pikeru.conf.

### Make applications use it
* Check `xdg_portal/pikeru.portal` and make sure the value of your `$XDG_CURRENT_DESKTOP` is in the `UseIn` section. Add it if not.
* Run `./install.sh` to install the xdg portal for pikeru.
* install.sh creates a symlink to this repo so if you want it to work for other users, put this repo in `/opt` first.
* If installing for multiple users, uncomment the block in `meson.build` that installs `portals.conf`, or copy `xdg_portal/contrib/portals.conf` to the ~/.config/xdg-desktop-portal/ directory of each user.
* If your chromium-based browser is not using xdg portal, you can still use pikeru by setting environment variable `XDG_CURRENT_DESKTOP=KDE` and symlinking the `kdialog` script in your path. That will trick the browser into thinking it's using the KDE dialog, assuming the real kdialog is not placed before this one in your path.
* To make firefox use the portal, set environment variable `GTK_USE_PORTAL=1`, and in `about:config`, set `widget.use-xdg-desktop-portal.file-picker` to `1`.
* The xdg portal should work for both Firefox and Chromium based browsers.
* The xdg portal is tested on arch and ubuntu. The kdialog hack should work anywhere.

## Special features
* Putting `postprocess.sh` in this directory enables post-processing, such as automatically converting images before uploading them. Use the example script with `cp postprocess.example.sh postprocess.sh`.
* `Cmd` menu shows commands specified in ~/.config/pikeru.conf. Click one to run it on the selected files.
* Right click an image to view it. Scroll the image to view the next and previous images.
* Select multiple directories with ctrl, shift, middle-click, or right-click. Click `Open` to view the contents of all selected directories at the same time.

## License
Pikeru is Public Domain.
