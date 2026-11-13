pkgbase=aurore
pkgname=('aurore-de' 'aurore-burzellaunch')
pkgver=0.1.0
pkgrel=1
pkgdesc="Arch Linux dynamic weather and time-based Wayland desktop environment"
arch=('x86_64')
license=('GPL-3.0-or-later')
makedepends=('cargo')

build() {
    cd ..
    cargo build --release
}

package_aurore-de() {
    depends=('wayfire' 'waybar' 'ttf-jetbrains-mono-nerd' 'pipewire-pulse' 'networkmanager')
    cd ..
    install -Dm755 target/release/aurore-de pkg/aurore-de/usr/bin/aurore-de
    install -Dm644 configs/wayfire.ini pkg/aurore-de/etc/xdg/aurore/wayfire.ini
    install -Dm644 configs/waybar.jsonc pkg/aurore-de/etc/xdg/aurore/waybar.jsonc
    install -Dm644 configs/waybar.css pkg/aurore-de/etc/xdg/aurore/waybar.css

    install -dm755 pkg/aurore-de/usr/share/wayland-sessions
    printf '[Desktop Entry]\nName=Aurore\nComment=Dynamic Weather and Time Wayland Desktop\nExec=wayfire -c /etc/xdg/aurore/wayfire.ini\nType=Application\n' > pkg/aurore-de/usr/share/wayland-sessions/aurore.desktop
}

package_aurore-burzellaunch() {
    depends=('fuzzel')
    cd ..
    install -Dm644 configs/burzellaunch.ini pkg/aurore-burzellaunch/etc/xdg/aurore/burzellaunch.ini
    install -dm755 pkg/aurore-burzellaunch/usr/bin
    printf '#!/bin/sh\nexec fuzzel --config /etc/xdg/aurore/burzellaunch.ini "$@"\n' > pkg/aurore-burzellaunch/usr/bin/aurore-burzellaunch
    chmod 755 pkg/aurore-burzellaunch/usr/bin/aurore-burzellaunch
}
