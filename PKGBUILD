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

  # Create directories
  install -dm755 "$pkgdir/usr/share/man/man5"
  install -dm755 "$pkgdir/usr/share/xdg-desktop-portal/portals"
  install -dm755 "$pkgdir/usr/share/dbus-1/services"
  install -dm755 "$pkgdir/usr/share/xdg-desktop-portal-pikeru"

  # Install binaries
  install -Dm755 "target/release/pikeru" "$pkgdir/usr/bin/pikeru"
  install -Dm755 "target/release/portal" "$pkgdir/usr/lib/xdg-desktop-portal-pikeru"

  # Install other files
  install -Dm755 "xdg_portal/pikeru-wrapper.sh" "$pkgdir/usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh"
  install -Dm644 "xdg_portal/xdg-desktop-portal-pikeru.service" "$pkgdir$(pkg-config --variable systemduserunitdir systemd)/xdg-desktop-portal-pikeru.service"
  install -Dm644 "xdg_portal/org.freedesktop.impl.portal.desktop.pikeru.service" "$pkgdir/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.pikeru.service"

  # Generate and install man page
  scdoc < "xdg_portal/xdg-desktop-portal-pikeru.5.scd" > "$pkgdir/usr/share/man/man5/xdg-desktop-portal-pikeru.5"

  # Generate and install portal file
  sed "s/@cur_desktop@/$(get_desktop)/" "xdg_portal/pikeru.portal.in" > "$pkgdir/usr/share/xdg-desktop-portal/portals/pikeru.portal"
}

