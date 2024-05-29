#!/bin/bash

# dest
unitdir=$(pkg-config --variable systemduserunitdir systemd)
portalbin=/usr/lib/xdg-desktop-portal-pikeru
bindir=/usr/local/bin
sharedir1=/usr/local/share/xdg-desktop-portal-pikeru
sharedir2=/usr/share/xdg-desktop-portal-pikeru
dbusdir1=/usr/local/share/dbus-1/services
dbusdir2=/usr/share/dbus-1/services
mandir=/usr/local/share/man/man5
portaldir=/usr/share/xdg-desktop-portal/portals
portalfile=/usr/share/xdg-desktop-portal/portals/pikeru.portal
confdir=$HOME/.config/xdg-desktop-portal-pikeru

# src
dbus_svc=xdg_portal/org.freedesktop.impl.portal.desktop.pikeru.service
pk_portal=xdg_portal/pikeru.portal.in
manpage=xdg_portal/xdg-desktop-portal-pikeru.5.scd
wrapper=xdg_portal/pikeru-wrapper.sh
sd_svc=xdg_portal/xdg-desktop-portal-pikeru.service
sample_conf=xdg_portal/config.sample

get_desktop(){
	[ -z "$XDG_CURRENT_DESKTOP" ] && return
	tail -n1 xdg_portal/pikeru.portal.in|grep -q $XDG_CURRENT_DESKTOP && return
	echo ";$XDG_CURRENT_DESKTOP"
}

if [[ $(whoami) = root ]]; then
	set -x
	mkdir -p $mandir $dbusdir1 $dbusdir2 $sharedir1 $sharedir2 $portaldir
	mv -u target/release/pikeru $bindir
	mv -u target/release/portal $portalbin
	cp -u $wrapper $sharedir1
	cp -u $wrapper $sharedir2
	cp -u $dbus_svc $dbusdir1
	cp -u $dbus_svc $dbusdir2
	cp -u $sd_svc $unitdir
	scdoc < $manpage > $mandir/xdg-desktop-portal-pikeru.5
	sed "s/@cur_desktop@/$(get_desktop)/" xdg_portal/pikeru.portal.in > $portalfile
else
	if ! command -v cargo &> /dev/null; then
		echo -e "Cargo is not installed. Enter 'y' to install it now\n(then press enter when prompted use default location)"
		read ans
		[ "$ans" = "y" ] || exit
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
		. ~/.cargo/env
	fi
	cargo build -r
	cargo build -r --bin portal
	mkdir -p $confdir
	[[ -r "$confdir/config" ]] || cp -u $sample_conf $confdir/config
	sudo "$0"
	set -x
	systemctl --user daemon-reload
	systemctl --user restart xdg-desktop-portal-pikeru.service || exit 1
	bash ./xdg_portal/setconfig.sh
fi
