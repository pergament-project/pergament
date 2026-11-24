## Installation

### Linux (x86_64)

Each release provides four package formats:

- **Debian/Ubuntu (.deb)**: `sudo apt install ./Pergament-X.Y.Z-linux-x86_64.deb`
  - Adds the Pergament apt repository for automatic updates
  - Repository signed with key `71162C8E5F5EFABAA6ECB400A8001EBD72C3A8BF`
  - Includes H.264 video decoder (`gstreamer1.0-libav`)

- **Fedora/RHEL (.rpm)**: `sudo dnf install ./Pergament-X.Y.Z-linux-x86_64.rpm`
  - Recommends `gstreamer1-libav` for video playback

- **AppImage**: `chmod +x Pergament-X.Y.Z-x86_64.AppImage && ./Pergament-X.Y.Z-x86_64.AppImage`
  - Supports AppImageUpdate via `.zsync` file
  - Includes H.264 video decoder

- **Flatpak**: `flatpak install --user Pergament-X.Y.Z-linux-x86_64.flatpak`
  - Runs against GNOME runtime (version pinned in manifest)

- **Zip (no package manager)**: Extract and run `./install.sh`
  - Installs to `~/.local` by default (`--prefix` to change)
  - Requires system dependencies: WebKitGTK 4.1, GTK 3, libsoup 3, OpenSSL, SQLite
  - Video needs `gst-libav` (`gstreamer1-libav` on Fedora)
  - **Video/audio playback from disk**: build with `--features gst-plugin` to include the Tauri GStreamer plugin (`gst-plugin-tauri`) for `asset://` URI support

### Windows (x86_64)

Extract `Pergament-X.Y.Z-windows-x86_64.zip` and run `pergament-app.exe`. The [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) is required (already installed on most Windows systems).

### macOS (Apple Silicon)

Open `Pergament-X.Y.Z-macos-arm64.dmg` and drag to Applications. **Not signed/notarized** — first start requires right-click → Open, or allow in System Settings → Privacy & Security. Not tested by the maintainer on a Mac.

---

## Verification

Check every download against the SHA-256 digest GitHub shows next to it.

---

## Notes

- **Linux video/audio from disk**: Playlist media files dropped from disk are served as `asset://` URIs. The Nix package includes the GStreamer plugin by default. Other packages (deb, rpm, AppImage, flatpak, zip) include it when built with `cargo build --release --features gst-plugin -p pergament-app`.
- The in-app updater shows only changes below the `<!-- changes -->` marker.