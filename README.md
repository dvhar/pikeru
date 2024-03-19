 # Pikeru: The File Picker with Good Thumbnails

Pikeru is a filepicker for linux that has working thumbnails and works on any desktop environment or window manager. Kind of like kdialog but better and with some added features.

![Screenshot_20240313_233010_resized](https://github.com/dvhar/pikeru/assets/33729230/eab08fc2-c10a-4a49-b561-d8a78ee263f9)

## Command Line Arguments
Pikeru takes several command line args and returns the selected file(s) to stdout separated by newlines.

- `-t title`: Sets the title displayed at the top of the Pikeru window.
- `-m mode`: Determines the mode of file selection operation:
  - `file`: Select a single file.
  - `files`: Select multiple files.
  - `dir`: Select a single directory.
  - `save`: Save a file with the specified filename. Prompt user if file already exists.
- `-p path`: Specifies the initial directory to display when Pikeru launches.
- `-i mime_list`: Defines a list of MIME types accepted during file selection.

Planned but not yet implemented:
- `-e windowId`: Specifies the X11 window ID of the parent window if Pikeru should be transient to an existing window.

## Installation and Usage

### Install the filepicker
* Install `tk` with your system package manager. Or whichever tkinter-related package python complains about when you run it.
* Make sure the python `venv` module is installed.
* Run `./pikeru`. That will create the venv and config file the first time and launch the filepicker.
* If using a high-dpi display, edit `dpi_scale` in ~/.config/pikeru.conf.

### Make applications use it
* The xdg-desktop-portal backend for pikeru is in the `xdg_portal` directory. Follow the readme there to install it. That should work for both Firefox and Chromium based browsers.
* If your chromium-based browser is not using xdg portal, you can still use pikeru by setting environment variable `XDG_CURRENT_DESKTOP=KDE` and symlinking the `kdialog` script in your path. That will trick the browser into thinking it's using the KDE dialog, assuming the real kdialog is not placed before this one in your path.

## License
Pikeru is Public Domain.
