 # Pikeru: The File Picker with Good Thumbnails

Pikeru is a filepicker for linux that has working thumbnails and works on any desktop environment or window manager. Kind of like kdialog but with some added features.

![screenshot](https://github.com/dvhar/pikeru/assets/33729230/652d10d7-d4da-4c21-bd3c-b64b87481cf0)

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

* Create a venv in this directory with `python -m venv venv` or set env `PK_VENV` to point to one. If using PK_VENV, make sure it's set for the browser and not just shell, or set it in run.sh.
* `. venv/bin/activate` and `pip install -r requirements.txt`. You may also need to install `tk` with your system package manager.
* To use with chromium-based browsers, set environment variable `XDG_CURRENT_DESKTOP=KDE` and symlink the `kdialog` script in your path. That will trick the browser into thinking it's using the KDE dialog.
* To use with Firefox and other programs that use xdg portal, install some xdg portal that uses kdialog or maybe try this: https://github.com/GermainZ/xdg-desktop-portal-termfilechooser . I haven't gotten it to work but maybe you can.

## License
Pikeru is Public Domain.
