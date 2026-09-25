#!/usr/bin/env bash
# Quita del AppImage las librerías de display que linuxdeploy copia de Ubuntu y que rompen el
# EGL del Mesa moderno (Fedora, Bazzite, Arch…): la ventana abría en blanco con
# «Could not create default EGL display: EGL_BAD_PARAMETER». Todas existen en cualquier
# escritorio Linux, así que se usan las del sistema. Ver tauri-apps/tauri#15976.
#
# Uso: scripts/fix-appimage.sh [tag]   (con tag, sube el AppImage arreglado al release)
set -euo pipefail

TAG="${1:-}"
APP=$(realpath $(ls src-tauri/target/release/bundle/appimage/*.AppImage | head -1))
WORK=$(mktemp -d)
export APPIMAGE_EXTRACT_AND_RUN=1

cp "$APP" "$WORK/in.AppImage"
chmod +x "$WORK/in.AppImage"
(cd "$WORK" && ./in.AppImage --appimage-extract >/dev/null)

for lib in libwayland-client.so.0 libwayland-cursor.so.0 libwayland-egl.so.1 libwayland-server.so.0 \
           libxkbcommon.so.0 libxcb-randr.so.0 libxcb-render.so.0 libxcb-shm.so.0 libXau.so.6 libXdmcp.so.6; do
  find "$WORK/squashfs-root" -name "$lib" -print -delete
done

curl -fsSL -o "$WORK/appimagetool" \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x "$WORK/appimagetool"
ARCH=x86_64 "$WORK/appimagetool" --no-appstream "$WORK/squashfs-root" "$APP"
echo "AppImage arreglado: $APP"

if [ -n "$TAG" ]; then
  # Se sube con el mismo nombre que ya tiene el asset del release para reemplazarlo.
  NAME=$(gh release view "$TAG" --json assets --jq '.assets[].name | select(endswith(".AppImage"))' | head -1)
  NAME="${NAME:-$(basename "$APP")}"
  cp "$APP" "$WORK/$NAME"
  gh release upload "$TAG" "$WORK/$NAME" --clobber
fi
