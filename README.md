 # Pikeru: The File Picker with Good Thumbnails

Pikeru is the only filepicker for linux that has working thumbnails and works on any desktop environment or window manager. Gtk will probably never implement thumbnails correctly at the toolkit level so a standalone program is needed.

![screenshot](screenshot.jpg)

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

## Usage

* Edit `run.sh` to point to your venv if using one, and edit the `kdialog` script to point to run.sh
* To use with chromium-based browsers, set environment variable XDG_CURRENT_DESKTOP=KDE and put the kdialog script in your path. That will trick the browser into thinking it's using the KDE dialog.
* To use with Firefox and other programs that use the xdg portal, maybe try this: https://github.com/GermainZ/xdg-desktop-portal-termfilechooser . I haven't gotten it to work but maybe you can.

## License
Pikeru is Public Domain.
