#!/bin/sh
[ -z "$XDG_CURRENT_DESKTOP" ] && exit
tail -n1 pikeru.portal.in|grep -q $XDG_CURRENT_DESKTOP && exit
echo ";$XDG_CURRENT_DESKTOP"
