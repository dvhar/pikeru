#!/bin/bash
# This configures the xdg portal for your currnet user to use pikeru. It does
# it by finding your previous portal config, copying it to a higher-precedence
# location, and adding or changing a line for pikeru.

xhome="${XDG_CONFIG_HOME:-$HOME/.config}"

# find config for xdg-desktop-portal
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

# backup old config if necessary
xdir="$xhome/xdg-desktop-portal"
portalfile="$xdir/portals.conf"
mkdir -p "$xdir"
if [ -f "$portalfile" ] && ! grep -q pikeru "$portalfile" ; then
	origconf="${portalfile}.orig"
	execute "mv '$portalfile' '$origconf'"
else
	origconf="$(findconf)"
fi

# create higher priority config for xdg-desktop-portal with pikeru as FileChooser
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

execute "systemctl --user restart xdg-desktop-portal.service"
echo -e "XDG portal has been configured to use pikeru. The config file for enabling pikeru portal is ${portalfile}.\nYou can revert it with 'pikeru -d' and re-enable pikeru with 'pikeru -e'"
