xdg-desktop-portal-pikeru(5)

# NAME

xdg-desktop-portal-pikeru - an xdg-desktop-portal backend to pick
files with the Pikeru file picker and create a searchable index for it

# DESCRIPTION

xdg-desktop-portal-pikeru allows applications to choose files via xdg-desktop-portal using your
favorite file picker, Pikeru.

# CONFIG AND PIKERU LAUNCHER LOCATIONS

The config file provided by a fresh installation is $XDG_CONFIG_HOME/xdg-desktop-portal-pikeru/config.
On startup, it will try to load the config file from these locations:

- $XDG_CONFIG_HOME/xdg-desktop-portal-pikeru/$XDG_CURRENT_DESKTOP
- $XDG_CONFIG_HOME/xdg-desktop-portal-pikeru/config
- /etc/xdg/xdg-desktop-portal-pikeru/$XDG_CURRENT_DESKTOP
- /etc/xdg/xdg-desktop-portal-pikeru/config

_$XDG_CONFIG_HOME_ defaults to _~/.config_.
_$XDG_CURRENT_DESKTOP_ can be a colon seperated list. Each element of that list will be tried.

The configuration file uses the INI file format. Example:

```
log_level = info

[filepicker]
cmd=/usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh
default_save_dir=~/Downloads
postprocess_dir=/tmp/pk_postprocess
postprocessor=/opt/pikeru/xdg_portal/postprocess.sh

[indexer]
enable = true
cmd = python /opt/pikeru/indexer/img_indexer.py http://10.0.0.10:7860/sdapi/v1/interrogate
check = curl http://10.0.0.10:7860/sdapi/v1/interrogate
extensions = png,jpg,jpeg,gif,webp,tiff,bmp

```

If no config is found, it will default to checking these locations for the
pikeru wrapper:

- /usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh
- /usr/local/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh
- /opt/pikeru/xdg_portal/contrib/pikeru-wrapper.sh

# CONFIG FILE OPTIONS

## GLOBAL OPTION

**log_level** = _off|error|warn|info|debug|trace_
	The level of detail the portal will log.

	Default value is info.

## FILEPICKER OPTIONS

These options need to be placed under the **[filepicker]** section. The "~"
chacacter will be expanded to your home directory.

**cmd** = _command_
	Command to execute for launching the filepicker. For invocation details,
	please refer to the pikeru-wrapper.sh script.

**default_save_dir** = _directory_
	Default directory to save file paths in, if the invoking program does not provide one.

	The default value is ~/Downloads.

**postprocess_dir** = _directory_
	Default directory to save post-processed files to when using a postprocess
	script. The xdg portal will avoid using this as the starting directory next time you
	select a file for upload.

	The default value is /tmp/pk_postprocess.

**postprocessor** = _script_
	The script to pipe selected files through before selecting them. This
	allows for automatic file conversions, resizing, compressing, etc. Takes
	lines of files as input and prints lines of files as output, which can be
	the same ones.

	This is empty by default, but an example is provided that you can uncomment.

## INDEXER OPTIONS

These options need to be placed under the **[indexer]** section. The "~"
chacacter will be expanded to your home directory. These values are used by the
portal to build an index that enables searching files by semantic content.

**enable** = _true/false_
	Turn semantic file indexing on or off.

	Default value is false.

**cmd** = _command_
	Command to execute for generating searchable text for a file. It will be
	given an additional filepath argument and will print the resulting text for
	that file to stdout.

	The default value is a script that calls a stable-diffusion-webui api
	endpoint to generate searchable text for images.

**check** = _command_
	Command to for checking whether or not the system used to generate
	searchable text is available. Must exit successfully if and only if the
	above cmd is expected to work successfully.

	Default value tests the same example stable diffusion server.

**extensions** = _comma separated list_
	List of file extension that the indexer command can process. Anything not in
	the list will not be indexed.

	Default value is a list of image formats that stable diffusion can handle.

# THE OTHER CONFIG FILE

The [SearchIgnore] section of pikeru's config file (~/.config/pikeru.conf) can
control which patterns you want to get indexed when visiting a directory. It
uses gitignore syntax.

# REMOTE CONTROL

**pikeru** has several command line flags for controlling the portal

pikeru -c
	Clear the indexing queue to skip processing any current files.

pikeru -d
	Configure xdg-desktop-portal to not use pikeru as the system file picker.

pikeru -e
	Configure xdg-desktop-portal to use pikeru as the system file picker.

## CAVEATS
```
-d and -e will not work if your xdg-desktop-portal version is older than
1.18. In that case, you will need to edit or remove
/usr/share/xdg-desktop-portal/portals/pikeru.portal to disable it. If you
want to keep the file but still disable it, remove your $XDG_CURRENT_DESKTOP
value from the `UseIn` section.
```
