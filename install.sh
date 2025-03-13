#!/bin/bash

if [ -f /etc/arch-release ]; then
    echo "Arch Linux detected!"
    if [ -f ./PKGBUILD ]; then
        read -p "PKGBUILD file found. Would you like to install using 'makepkg -si' instead? (y/n): " choice
        case "$choice" in
            y|Y) echo "Running makepkg -si..."
                makepkg -si
                exit 0 ;;
            *) echo "Continuing with the regular installation script..." ;;
        esac
    else
        echo "No PKGBUILD file found in the current directory. Continuing with regular installation..."
    fi
fi

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
	cp -u xdg_portal/pikeru-wrapper.sh $sharedir1
	cp -u xdg_portal/pikeru-wrapper.sh $sharedir2
	cp -u xdg_portal/postprocess.example.sh $sharedir2
	cp -u indexer/img_indexer.py $sharedir2
	cp -u xdg_portal/org.freedesktop.impl.portal.desktop.pikeru.service $dbusdir1
	cp -u xdg_portal/org.freedesktop.impl.portal.desktop.pikeru.service $dbusdir2
	cp -u xdg_portal/xdg-desktop-portal-pikeru.service $unitdir
	scdoc < xdg_portal/xdg-desktop-portal-pikeru.5.scd > $mandir/xdg-desktop-portal-pikeru.5
	sed "s/@cur_desktop@/$(get_desktop)/" xdg_portal/pikeru.portal.in > $portalfile
else
	this_dir="$(dirname $0)"
	cd "$this_dir"
	if ! command -v cargo &> /dev/null; then
		echo -e "Cargo is not installed. Enter 'y' to install it now\n(then press enter when prompted use default location)"
		read ans
		[ "$ans" = "y" ] || exit
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
		. ~/.cargo/env
	fi
	cargo build -r || exit 1
	cargo build -r --bin portal || exit 1
	mkdir -p $confdir
	sudo "$0"
	set -x
	systemctl --user daemon-reload
	bash ./xdg_portal/setconfig.sh
fi
