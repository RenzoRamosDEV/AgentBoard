#!/usr/bin/env bash
# Renombra los instaladores del release a AgentBoard-App_<versión>_<sistema>_<arch>.<ext> para
# distinguirlos del servidor MCP suelto (AgentBoard-MCP_<versión>_<sistema>).
# Uso: scripts/rename-release-assets.sh <tag>
set -euo pipefail
TAG="$1"; V="${TAG#v}"
REPO="${GITHUB_REPOSITORY:-RenzoRamosDEV/AgentBoard}"

gh release view "$TAG" --repo "$REPO" --json assets --jq '.assets[] | "\(.apiUrl)\t\(.name)"' |
while IFS=$'\t' read -r url name; do
  case "$name" in
    AgentBoard-App_*|AgentBoard-MCP_*) continue ;;
    *.AppImage)       new="AgentBoard-App_${V}_linux_x86_64.AppImage" ;;
    *.deb)            new="AgentBoard-App_${V}_linux_x86_64.deb" ;;
    *.rpm)            new="AgentBoard-App_${V}_linux_x86_64.rpm" ;;
    *-setup.exe)      new="AgentBoard-App_${V}_windows_x64-setup.exe" ;;
    *.msi)            new="AgentBoard-App_${V}_windows_x64.msi" ;;
    *.dmg)            new="AgentBoard-App_${V}_macos_universal.dmg" ;;
    *.app.tar.gz)     new="AgentBoard-App_${V}_macos_universal.app.tar.gz" ;;
    *) continue ;;
  esac
  gh api -X PATCH "$url" -f name="$new" --jq '"\(.name)"'
done
