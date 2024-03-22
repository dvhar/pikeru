#!/bin/bash

cd xdg_portal
homeconfig=~/.config/xdg-desktop-portal
mkdir -p $homeconfig
cp -u contrib/portals.conf $homeconfig

meson setup \
  --prefix        /usr \
  --libexecdir    lib \
  --sbindir       bin \
  --buildtype     plain \
  --auto-features enabled \
  --wrap-mode     nodownload \
  -D              b_pie=true \
  -Dsd-bus-provider=libsystemd build

ninja -C build
ninja -C build install
systemctl --user daemon-reload
systemctl --user restart xdg-desktop-portal-pikeru.service
systemctl --user restart xdg-desktop-portal.service
