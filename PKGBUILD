# Maintainer: Nekota-san
pkgbase=aurore
pkgname=('aurore-de' 'aurore-burzellaunch')
pkgver=0.1.0
pkgrel=1
pkgdesc="Arch Linux dynamic weather and time-based Wayland desktop environment"
arch=('x86_64')
url="https://github.com/ALBAlinux-project/Aurore"
license=('GPL-3.0-or-later')
makedepends=('cargo')

build() {
    cd "$startdir"
    RUSTFLAGS="-C link-arg=-fuse-ld=bfd" cargo build --release
}
package_aurore-de() {
    pkgdesc="Aurore desktop environment (Wayfire + Waybar, weather-driven color)"
    depends=('wayfire' 'waybar' 'ttf-jetbrains-mono-nerd' 'pipewire-pulse' 'networkmanager')

    install -Dm755 "$startdir/target/release/aurore-de" "$pkgdir/usr/bin/aurore-de"
    install -Dm644 "$startdir/configs/wayfire.ini"   "$pkgdir/etc/xdg/aurore/wayfire.ini"
    install -Dm644 "$startdir/configs/waybar.jsonc"  "$pkgdir/etc/xdg/aurore/waybar.jsonc"
    install -Dm644 "$startdir/configs/waybar.css"    "$pkgdir/etc/xdg/aurore/waybar.css"

    install -dm755 "$pkgdir/usr/share/wayland-sessions"
    printf '[Desktop Entry]\nName=Aurore\nComment=Dynamic Weather and Time Wayland Desktop\nExec=wayfire -c /etc/xdg/aurore/wayfire.ini\nType=Application\n' \
        > "$pkgdir/usr/share/wayland-sessions/aurore.desktop"
}

package_aurore-burzellaunch() {
    pkgdesc="Launcher for Aurore (fuzzel wrapper)"
    depends=('fuzzel')

    install -Dm644 "$startdir/configs/burzellaunch.ini" "$pkgdir/etc/xdg/aurore/burzellaunch.ini"
    install -dm755 "$pkgdir/usr/bin"
    printf '#!/bin/sh\nexec fuzzel --config "${XDG_CACHE_HOME:-$HOME/.cache}/aurore/burzellaunch.ini" "$@"\n' \
        > "$pkgdir/usr/bin/aurore-burzellaunch"
    chmod 755 "$pkgdir/usr/bin/aurore-burzellaunch"
}
