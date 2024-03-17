# xdg-desktop-portal-pikeru

xdg-desktop-portal backend for picking files with pikeru.

## Building and Installing

* `./install.sh`
* This works on arch, other distros may need some work
* This creates a symlink to this repo so if you want it to work for other users, put this repo in `/opt` first.
* If installing for multiple users, you can uncomment the block in `meson.build` that installs `portals.conf`

## Running

The install script will get the portal up and running but firefox needs some extra configuration to use it.
* set environment variable `GTK_USE_PORTAL=1`.
* in `about:config`, set `widget.use-xdg-desktop-portal.file-picker` to `1`


## Manual startup

At the moment, some command line flags are available for development and
testing. If you need to use one of these flags, you can start an instance of
xdpp using the following command:

```sh
xdg-desktop-portal-pikeru -r [OPTION...]
```

To list the available options, you can run `xdg-desktop-portal-pikeru
--help`.

## License

MIT

This work is based on these:
- [xdg-desktop-portal-termfilechooser](https://github.com/GermainZ/xdg-desktop-portal-termfilechooser)
- [xdg-desktop-portal](https://github.com/flatpak/xdg-desktop-portal)
- [xdg-desktop-portal-wlr](https://github.com/emersion/xdg-desktop-portal-wlr)
