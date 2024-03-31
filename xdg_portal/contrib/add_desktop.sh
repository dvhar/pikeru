#!/bin/sh
# Used by meson to add your desktop to pikeru.portal if needed
[ -z "$XDG_CURRENT_DESKTOP" ] && exit
tail -n1 pikeru.portal|grep -q $XDG_CURRENT_DESKTOP && exit
echo ";$XDG_CURRENT_DESKTOP"
