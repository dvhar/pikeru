#!/bin/bash
xconfig="${XDG_CONFIG_HOME:-$HOME/.config}/xdg-desktop-portal"
execute(){
    echo "$*"
    eval "$*"
}
if [ -r $xconfig/portals.conf.orig ]; then
    execute "mv $xconfig/portals.conf.orig $xconfig/portals.conf"
elif [ ! -r "$xconfig/portals.conf" ] || ! grep -q pikeru "$xconfig/portals.conf"; then
    echo -e "XDG portal is not configured to use pikeru. Not changing anything."
    echo "You can enable pikeru with -x"
    exit
else
    execute "rm $xconfig/portals.conf"
fi
execute "systemctl --user restart xdg-desktop-portal.service"
echo 'Pikeru filepicker disabled. Re-enable with -x'
