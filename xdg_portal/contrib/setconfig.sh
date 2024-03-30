#!/bin/bash
# This configures the xdg portal for your currnet user to use pikeru.
# It does it by finding the portal config you're currently using, copying it to
# a higher-precedence location, and adding or changing a line for pikeru.

xhome="${XDG_CONFIG_HOME:-$HOME/.config}"

findconf(){
	dt=${XDG_CURRENT_DESKTOP,,}
	IFS=: read -r dt1 dt2 <<< "$dt"
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
		b="${dir}/xdg-desktop-portal/${dt1}-portals.conf"
		c="${dir}/xdg-desktop-portal/${dt2}-portals.conf"
		[[ -f "$a" ]] && echo "$a" && return 0
		[[ -f "$b" ]] && echo "$b" && return 0
		[[ -f "$c" ]] && echo "$c" && return 0
	done
	return 1
}

xdir="$xhome/xdg-desktop-portal"
portalfile="$xdir/portals.conf"
mkdir -p "$xdir"

if [[ -f "$portalfile" ]]; then
	mv "$portalfile" "${portalfile}.orig"
	origconf="${portalfile}.orig"
else
	origconf="$(findconf)"
fi

if [[ ! -z "$origconf" ]]; then
	sed '/FileChooser/d' "$origconf" > "$portalfile"
	echo 'org.freedesktop.impl.portal.FileChooser=pikeru' >> "$portalfile"
else
cat << EOF > "$portalfile"
[preferred]
default=auto
org.freedesktop.impl.portal.FileChooser=pikeru
EOF
fi

[[ "$origconf" =~ orig$ ]] && how="renaming $origconf to $(basename $portalfile)" || how='deleting it'
echo -e "Your new xdg-desktop-portal config is $portalfile.\nYou can revert by ${how}"

systemctl --user restart xdg-desktop-portal.service
