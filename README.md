 # Pikeru: The File Picker with Good Thumbnails

Pikeru is a filepicker for linux that has working thumbnails and works on any desktop environment or window manager. Kind of like kdialog but better and with some added features.

![Screenshot_20240313_233010_resized](https://github.com/dvhar/pikeru/assets/33729230/eab08fc2-c10a-4a49-b561-d8a78ee263f9)

## Installation and Usage

### Install the filepicker
* Install `tk` with your system package manager. Or whichever tkinter-related package python complains about when you run it.
* Make sure the python `venv` module is installed.
* Run `./pikeru`. That will create the venv and config file the first time and launch the filepicker.
* If using a high-dpi display, edit `dpi_scale` in ~/.config/pikeru.conf.

### Make applications use it
* The xdg-desktop-portal backend for pikeru is in the `xdg_portal` directory. Follow the readme there to install it. That should work for both Firefox and Chromium based browsers.
* If your chromium-based browser is not using xdg portal, you can still use pikeru by setting environment variable `XDG_CURRENT_DESKTOP=KDE` and symlinking the `kdialog` script in your path. That will trick the browser into thinking it's using the KDE dialog, assuming the real kdialog is not placed before this one in your path.
* The xdg portal should run on any distro but some of them can be tricky to configure to use it. Make a pull request if yours requires some tinkering to make it work. The kdialog hack should work anywhere.

### Command Line Arguments
Pikeru takes several command line args and returns the selected file(s) to stdout separated by newlines.

- `-t title`: Sets the title displayed at the top of the Pikeru window.
- `-m mode`: Determines the mode of file selection operation:
  - `file`: Select a single file.
  - `files`: Select multiple files.
  - `dir`: Select a single directory.
  - `save`: Save a file with the filename specified with -p
- `-p path`: Specifies the initial directory or filename.
- `-i mime_list`: Space-separated list of MIME types to display.
- `-u`: Update python dependencies. Useful if you did a git pull and a new dep was added.

Planned but not yet implemented:
- `-e windowId`: Specifies the X11 window ID of the parent window if Pikeru should be transient to an existing window.

## License
Pikeru is Public Domain.
