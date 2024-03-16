#!/bin/bash

homeconfig=~/.config/xdg-desktop-portal
mkdir -p $homeconfig
cp contrib/portals.conf $homeconfig

meson setup \
  --prefix        /usr \
  --libexecdir    lib \
  --sbindir       bin \
  --buildtype     plain \
  --auto-features enabled \
  --wrap-mode     nodownload \
  -D              b_pie=true \
  -D              python.bytecompile=1 \
  -Dsd-bus-provider=libsystemd build

ninja -C build
ninja -C build install
systemctl --user daemon-reload
systemctl --user restart xdg-desktop-portal.service
