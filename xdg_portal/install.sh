#!/bin/bash

homeconfig=~/.config/xdg-desktop-portal
mkdir -p $homeconfig
cp contrib/portals.conf $homeconfig

#meson build
arch-meson -Dsd-bus-provider=libsystemd build

ninja -C build
ninja -C build install
systecmtl --user daemon-reload
systecmtl --user restart xdg-desktop-portal.service
