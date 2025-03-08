# NOTE: this PKGBUILD is under construction and probably not ready to use yet

pkgname=pikeru
pkgver=1.0
pkgrel=1
pkgdesc="A file picker with proper thumbnails and search"
arch=('x86_64')
url="https://github.com/dvhar/pikeru"
license=('MIT')
depends=('ffmpeg', 'xdg-desktop-portal')
makedepends=('cargo', 'clang', 'scdoc')
source=("$pkgname-$pkgver.tar.gz::https://github.com/dvhar/$pkgname/archive/refs/tags/v$pkgver.tar.gz")
sha512sums=('5aad8f6821efc6b1863f65a2b84eb359f185e627ea97d27326bd5d5d9a114607512999ef317ba381007752fec6c16df50539a3b0091847efb18a44fa00247259')
options=()

build() {
  cd "$pkgname-$pkgver"
  cargo build --release --locked
  cargo build --release --locked --bin portal
}

get_desktop(){
    [ -z "$XDG_CURRENT_DESKTOP" ] && return
    tail -n1 xdg_portal/pikeru.portal.in|grep -q $XDG_CURRENT_DESKTOP && return
    echo ";$XDG_CURRENT_DESKTOP"
}

package() {
  cd "$pkgname-$pkgver"

  mandir="$pkgdir/usr/local/share/man/man5"
  portaldir="/usr/share/xdg-desktop-portal/portals"
  dbusdir="/usr/local/share/dbus-1/services"
  sharedir="/usr/local/share/xdg-desktop-portal-pikeru"
  mkdir -p $mandir $portaldir $dbusdir $sharedir

  install -Dm755 "target/release/pikeru" "$pkgdir/usr/local/bin/pikeru"
  install -Dm755 "target/release/portal" "$pkgdir/usr/local/bin/xdg-desktop-portal-pikeru"
  install -Dm755 "xdg_portal/pikeru-wrapper.sh" "$pkgdir/$sharedir"
  install -Dm644 "xdg_portal/xdg-desktop-portal-pikeru.service" $(pkg-config --variable systemduserunitdir systemd)
  install -Dm644 "xdg_portal/org.freedesktop.impl.portal.desktop.pikeru.service" "$pkgdir/$dbusdir"
  scdoc < "xdg_portal/xdg-desktop-portal-pikeru.5.scd" > "$pdgdir/$mandir/xdg-desktop-portal-pikeru.5"
	sed "s/@cur_desktop@/$(get_desktop)/" "xdg_portal/pikeru.portal.in" > "$pkgdir/usr/share/xdg-desktop-portal/portals/pikeru.portal"

}
