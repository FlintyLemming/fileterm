#!/bin/bash
# Prepare a Claude Code cloud session to build and run the Linux GUI (Tauri) app.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}"

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  SUDO="sudo"
fi

# Tauri/WebKitGTK build deps (mirrors .github/workflows/ci.yml) plus a virtual
# X display, screenshot tools and CJK fonts for headless GUI previews, and
# OpenSSH for the real-sshd Rust tests.
APT_PACKAGES=(
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf
  xvfb xauth dbus-x11 x11-utils imagemagick fonts-noto-cjk
  openssh-server openssh-client
)
missing=()
for pkg in "${APT_PACKAGES[@]}"; do
  dpkg-query -W -f='${Status}' "$pkg" 2>/dev/null | grep -q "install ok installed" || missing+=("$pkg")
done
if [ "${#missing[@]}" -gt 0 ]; then
  $SUDO apt-get update -qq
  DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq "${missing[@]}"
fi
# sshd refuses to start without its privilege-separation directory, which the
# container does not create (no systemd tmpfiles).
$SUDO mkdir -p /run/sshd

# JS workspace deps and the shared packages the renderer imports.
npm install --no-audit --no-fund
npm run build:packages

# Warm the Rust dependency cache so later builds only compile FileTerm itself.
cargo fetch --locked --manifest-path apps/tauri/src-tauri/Cargo.toml
