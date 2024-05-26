#!/bin/bash

# dest
unitdir=$(pkg-config --variable systemduserunitdir systemd)
portalbin=/usr/local/libexec/xdg-desktop-portal-pikeru
bindir=/usr/local/bin
sharedir=/usr/local/share
dbusdir1=/usr/local/share/dbus-1/services
dbusdir2=/usr/share/dbus-1/services
mandir=/usr/local/share/man/man5
portalfile=/usr/share/xdg-desktop-portal/portals/pikeru.portal
portal_conf=$HOME/.config/xdg-desktop-portal-pikeru/config

# src
dbus_svc=xdg_portal/org.freedesktop.impl.portal.desktop.pikeru.service
pk_portal=xdg_portal/pikeru.portal.in
manpage=xdg_portal/xdg-desktop-portal-pikeru.5.scd
wrapper=xdg_portal/pikeru-wrapper.sh
sd_svc=xdg_portal/xdg-desktop-portal-pikeru.service
sample_conf=xdg_portal/config.sample

if [[ $(whoami) = root ]]; then
	set -x
	mkdir -p $mandir $dbusdir1 $dbusdir2
	mv target/release/pikeru $bindir
	mv target/release/portal $portalbin
	cp $wrapper $sharedir
	cp $dbus_svc $dbusdir1
	cp $dbus_svc $dbusdir2
	cp $sd_svc $unitdir
	scdoc < $manpage > $mandir/xdg-desktop-portal-pikeru.5
	sed "s/@cur_desktop@/$(sh xdg_portal/add_desktop.sh)/" \
		xdg_portal/pikeru.portal.in > $portalfile
else
	cargo build -r
	cargo build -r --bin portal
	[[ -r "$portal_conf" ]] || cp $sample_conf $portal_conf
	sudo "$0"
	set -x
	systemctl --user daemon-reload
	systemctl --user restart xdg-desktop-portal-pikeru.service || exit 1
	bash ./contrib/setconfig.sh
fi
