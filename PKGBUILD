# NOTE: this PKGBUILD is under construction and probably not ready to use yet

pkgname=pikeru
pkgver=1.1
pkgrel=1
pkgdesc="A file picker with proper thumbnails and search"
arch=('x86_64')
url="https://github.com/dvhar/pikeru"
license=('MIT')
depends=('ffmpeg', 'xdg-desktop-portal')
makedepends=('cargo', 'clang', 'scdoc')
source=("$pkgname-$pkgver.tar.gz::https://github.com/dvhar/$pkgname/archive/refs/tags/$pkgver.tar.gz")
sha512sums=('89a9445f76c56f02b9ac4b245f0cc8865e365a999b7c3e6ef1a54c0d357157f7bbc555e704d1a5da8c8c916cf4212f3fe0afe6efee36fef659a93276ac4d9952')
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

