# Maintainer: dvhar <jnk at davosaur dot com>

pkgname=pikeru
pkgver=1.5
pkgrel=1
pkgdesc="A system file picker with proper thumbnails and search"
arch=('x86_64')
url="https://github.com/dvhar/pikeru"
license=('MIT')
depends=('ffmpeg' 'xdg-desktop-portal' 'sqlite' 'poppler')
makedepends=('cargo' 'clang' 'scdoc')
optdepends=('epub-thumbnailer-git: epub thumbnail support')
source=("$pkgname-$pkgver.tar.gz::https://github.com/dvhar/$pkgname/archive/refs/tags/$pkgver.tar.gz")
sha512sums=('681ff8b9e0423fe726dd1e4aece1110c5e9f032c6323633dc547dc1e12f489c8e530c27f14349729becd0f6a665e8e3965e685d2dbbe67b2e339696dfe1239fd')
options=(!debug !lto)
install="${pkgname}.install"

build() {
  cd "$pkgname-$pkgver"
  cargo build --release --locked
  cargo build --release --locked --bin portal
}

_get_desktop(){
    [ -z "$XDG_CURRENT_DESKTOP" ] && return
    tail -n1 xdg_portal/pikeru.portal.in|grep -q $XDG_CURRENT_DESKTOP && return
    echo ";$XDG_CURRENT_DESKTOP"
}

package() {
  cd "$pkgname-$pkgver"

  install -dm755 "$pkgdir/usr/share/man/man5"
  install -dm755 "$pkgdir/usr/share/xdg-desktop-portal/portals"
  install -Dm755 "target/release/pikeru" "$pkgdir/usr/bin/pikeru"
  install -Dm755 "target/release/portal" "$pkgdir/usr/lib/xdg-desktop-portal-pikeru"
  install -Dm755 "xdg_portal/pikeru-wrapper.sh" "$pkgdir/usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh"
  install -Dm755 "xdg_portal/postprocess.example.sh" "$pkgdir/usr/share/xdg-desktop-portal-pikeru/postprocess.example.sh"
  install -Dm755 "xdg_portal/setconfig.sh" "$pkgdir/usr/share/xdg-desktop-portal-pikeru/setconfig.sh"
  install -Dm755 "xdg_portal/unsetconfig.sh" "$pkgdir/usr/share/xdg-desktop-portal-pikeru/unsetconfig.sh"
  install -Dm755 "indexer/img_indexer.py" "$pkgdir/usr/share/xdg-desktop-portal-pikeru/img_indexer.py"
  install -Dm644 "xdg_portal/xdg-desktop-portal-pikeru.service" "$pkgdir$(pkg-config --variable systemduserunitdir systemd)/xdg-desktop-portal-pikeru.service"
  install -Dm644 "xdg_portal/org.freedesktop.impl.portal.desktop.pikeru.service" "$pkgdir/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.pikeru.service"
  scdoc < "xdg_portal/xdg-desktop-portal-pikeru.5.scd" > "$pkgdir/usr/share/man/man5/xdg-desktop-portal-pikeru.5"
  sed "s/@cur_desktop@/$(_get_desktop)/" "xdg_portal/pikeru.portal.in" > "$pkgdir/usr/share/xdg-desktop-portal/portals/pikeru.portal"
}

