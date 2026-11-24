#!/usr/bin/env bash
# Pack the release build into target/package/pergament-X.Y.Z-linux-<arch>.zip.
# Build first:
#   bun run build
#   cargo build --release -p pergament -p pergament-app --features pergament-app/custom-protocol,pergament-app/gst-plugin
set -euo pipefail
die() { echo "package: $*" >&2; exit 1; }

version=${1:?usage: scripts/package.sh X.Y.Z}
cd "$(git rev-parse --show-toplevel)"
scripts/versions.sh "$version" >/dev/null

arch=$(uname -m)
name=pergament-$version-linux-$arch
out=target/package
stage=$out/$name

for f in target/release/pergament target/release/pergament-app; do
    [[ -x $f ]] || die "$f is missing; build the release first"
done

# Build the gst-plugin-tauri if the feature is enabled
plugin_src="target/release/libgsttauriasset.so"
plugin_real="target/release/deps/libgsttauriasset-*.so"
if [[ -f $plugin_src ]]; then
    echo "package: found gst-plugin-tauri at $plugin_src"
elif ls $plugin_real 1> /dev/null 2>&1; then
    echo "package: found gst-plugin-tauri at $plugin_real, copying..."
    cp $(ls $plugin_real | head -1) $plugin_src
else
    echo "package: gst-plugin-tauri not found, building with gst-plugin feature..."
    cargo build --release -p pergament-app --features gst-plugin 2>&1 | tail -5
    if ls $plugin_real 1> /dev/null 2>&1; then
        cp $(ls $plugin_real | head -1) $plugin_src
    else
        die "gst-plugin-tauri build failed"
    fi
fi

rm -rf "$stage" "$out/$name.zip"
mkdir -p "$stage/bin" "$stage/lib/gstreamer-1.0" "$stage/share/icons"
install -m755 target/release/pergament target/release/pergament-app "$stage/bin/"
install -m755 "$plugin_src" "$stage/lib/gstreamer-1.0/"
for size in 32 64 128 256; do
    install -m644 "src-tauri/icons/${size}x${size}.png" "$stage/share/icons/"
done
install -m644 src-tauri/icons/icon.png "$stage/share/icons/512x512.png"
install -m644 src-tauri/icons/icon.svg "$stage/share/icons/"
install -m755 scripts/bundle-install.sh "$stage/install.sh"
install -m644 LICENSE README.md "$stage/"

(cd "$out" && zip -qr -X "$name.zip" "$name")
rm -rf "$stage"
echo "package: $out/$name.zip"
