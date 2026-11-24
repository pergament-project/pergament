## Notes

- **Linux video/audio from disk**: Playlist media files dropped from disk are served as `asset://` URIs. The Nix package includes the GStreamer plugin by default. Other packages (deb, rpm, AppImage, flatpak, zip) include it when built with `cargo build --release --features gst-plugin -p pergament-app`.
- The in-app updater shows only changes below the `<!-- changes -->` marker.