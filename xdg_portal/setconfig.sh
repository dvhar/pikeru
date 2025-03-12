#!/bin/bash
# This configures the xdg portal for your currnet user to use pikeru. It does
# it by finding your previous portal config, copying it to a higher-precedence
# location, and adding or changing a line for pikeru.

xhome="${XDG_CONFIG_HOME:-$HOME/.config}"

findconf(){
	xcd=${XDG_CURRENT_DESKTOP,,}
	desktops=(${xcd//:/ })
	dirs=(
		${xhome}
		${XDG_CONFIG_DIRS//:/ }
		/etc/xdg
		/etc
		${XDG_DATA_HOME:-$HOME/.local/share}
		${XDG_DATA_DIRS//:/ }
		/usr/local/share
		/usr/share
	)
	for dir in "${dirs[@]}"; do
		a="${dir}/xdg-desktop-portal/portals.conf"
		[ -f "$a" ] && ! grep -q pikeru "$a" && echo "$a" && return 0
		for dt in "${desktops[@]}"; do
			b="${dir}/xdg-desktop-portal/${dt}-portals.conf"
			[ -f "$b" ] && ! grep -q pikeru "$b" && echo "$b" && return 0
		done
	done
	return 1
}

execute(){
    echo "$*"
    eval "$*"
}

xdir="$xhome/xdg-desktop-portal"
portalfile="$xdir/portals.conf"
mkdir -p "$xdir"

if [ -f "$portalfile" ] && ! grep -q pikeru "$portalfile" ; then
	origconf="${portalfile}.orig"
	execute "mv '$portalfile' '$origconf'"
else
	origconf="$(findconf)"
fi

if [ ! -z "$origconf" ]; then
	execute "sed '/FileChooser/d' '$origconf' > '$portalfile'"
	execute "echo 'org.freedesktop.impl.portal.FileChooser=pikeru' >> '$portalfile'"
else
execute "cat << EOF > '$portalfile'
[preferred]
default=auto
org.freedesktop.impl.portal.FileChooser=pikeru
EOF
"
fi

xdgpconf="$HOME/.config/xdg-desktop-portal-pikeru"
mkdir -p "$xdgpconf"
[[ -r "$xdgpconf/config" ]] || cat << EOF > $xdgpconf/config
# off, error, warn, info, debug, trace
log_level = info

[filepicker]
cmd=/usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh
default_save_dir=~/Downloads

# Point postprocessor to a script to automatically process files before upload.
# Replace the empty config value with the commented one to use the example script.
#postprocessor=/usr/share/xdg-desktop-portal-pikeru/postprocess.example.sh
postprocessor=
postprocess_dir=/tmp/pk_postprocess


[indexer]
# this section tells xdg-desktop-portal-pikeru how to build an index for semantic search.
# The example values here are for a stable-diffusion-webui server running on localhost that
# is used to generate searchable text for files in any directory opened by pikeru.
# Set log_level above to trace to see the searchable text results.

enable = false

# bash command that will be given an additional filepath arg and prints searchable text to stdout.
cmd = python /usr/share/xdg-desktop-portal-pikeru/img_indexer.py http://127.0.0.1:7860/sdapi/v1/interrogate

# bash command that only returns status code 0 when the indexer is online
check = curl http://127.0.0.1:7860/sdapi/v1/interrogate

# comma-separate list of file types that 'cmd' can process.
extensions = png,jpg,jpeg,gif,webp,tiff,bmp
EOF

execute "systemctl --user restart xdg-desktop-portal.service"
echo -e "XDG portal has been configured to use pikeru. Config file is ${portalfile}.\nYou can revert it with 'pikeru -d' and re-enable pikeru with 'pikeru -e'"
