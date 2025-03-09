# NOTE: this PKGBUILD is under construction and probably not ready to use yet

pkgname=pikeru
pkgver=1.2
pkgrel=1
pkgdesc="A file picker with proper thumbnails and search"
arch=('x86_64')
url="https://github.com/dvhar/pikeru"
license=('MIT')
depends=('ffmpeg' 'xdg-desktop-portal' 'sqlite')
makedepends=('cargo' 'clang' 'scdoc')
source=("$pkgname-$pkgver.tar.gz::https://github.com/dvhar/$pkgname/archive/refs/tags/$pkgver.tar.gz")
sha512sums=('0ad1da29313b55f2f0435ef1967c0e10c2be9f8bd8b4f0521259516d2f16320b7385c96fa92b7617aa3d9c2de4e16ddfa7b5af4cb7a3b72e82f9f712d059fcc8')
options=()

build() {
  cd "$pkgname-$pkgver"
  unset LDFLAGS
  unset FCFLAGS
  unset CFLAGS
  unset RUSTFLAGS
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
  install -Dm755 "xdg_portal/postprocess.example.sh" "$pkgdir/usr/share/xdg-desktop-portal-pikeru/postprocess.example.sh"
  install -Dm755 "indexer/img_indexer.py" "$pkgdir/usr/share/xdg-desktop-portal-pikeru/img_indexer.py"
  install -Dm644 "xdg_portal/xdg-desktop-portal-pikeru.service" "$pkgdir$(pkg-config --variable systemduserunitdir systemd)/xdg-desktop-portal-pikeru.service"
  install -Dm644 "xdg_portal/org.freedesktop.impl.portal.desktop.pikeru.service" "$pkgdir/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.pikeru.service"

  # Generate and install man page
  scdoc < "xdg_portal/xdg-desktop-portal-pikeru.5.scd" > "$pkgdir/usr/share/man/man5/xdg-desktop-portal-pikeru.5"

  # Generate and install portal file
  sed "s/@cur_desktop@/$(get_desktop)/" "xdg_portal/pikeru.portal.in" > "$pkgdir/usr/share/xdg-desktop-portal/portals/pikeru.portal"
}

