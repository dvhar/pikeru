#!/bin/bash

cd xdg_portal

scmd="\$ s/\$/;$XDG_CURRENT_DESKTOP/"
tail -n1 pikeru.portal|grep -q $XDG_CURRENT_DESKTOP||sed -i "$scmd" pikeru.portal

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

exe=/usr/local/bin/pikeru
[ -f "$exe" ] || sudo ln -s `realpath ../pikeru` $exe

systemctl --user daemon-reload
systemctl --user restart xdg-desktop-portal-pikeru.service
echo '-----------------------------'
bash ./contrib/setconfig.sh
