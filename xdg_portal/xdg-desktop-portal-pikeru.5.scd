xdg-desktop-portal-pikeru(5)

# NAME

xdg-desktop-portal-pikeru - an xdg-desktop-portal backend to choose
files with the Pikeru file picker

# DESCRIPTION

xdg-desktop-portal-pikeru (or xdpp for short) allows applications to
choose files via xdg-desktop-portal using your favorite filepiker, Pikeru.

xdpp will try to load the configuration file from these locations before going with settings:

- $XDG_CONFIG_HOME/xdg-desktop-portal-pikeru/$XDG_CURRENT_DESKTOP
- $XDG_CONFIG_HOME/xdg-desktop-portal-pikeru/config
- /etc/xdg/xdg-desktop-portal-pikeru/$XDG_CURRENT_DESKTOP
- /etc/xdg/xdg-desktop-portal-pikeru/config

_$XDG_CONFIG_HOME_ defaults to _~/.config_.
_$XDG_CURRENT_DESKTOP_ can be a colon seperated list. Each element of that list will be tried.

The configuration files use the INI file format. Example:

```
[filechooser]
cmd=/usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh
```

If no config is found, it will default to checking these locations for the
pikeru wrapper:

- /usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh
- /usr/local/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh

# FILECHOOSER OPTIONS

These options need to be placed under the **[filechooser]** section.

**cmd** = _command_
	Command to execute. For invocation details, please refer to the wrapper script.

	The wrapper script is a symlink to contrib/pikeru-wrapper.sh, and is
	configured to launch the pikeru file picker to select files.

**default_dir** = _directory_
	Default directory to save file paths in, if the invoking program does not
	provide one.

	The default value is /tmp.

	This option will probably be deprecated in the future.
