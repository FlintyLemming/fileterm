#!/bin/bash
# Build (if needed) and launch the FileTerm Linux GUI on a virtual X display,
# then save a screenshot. Intended for headless cloud sessions.
#
# Usage: .claude/scripts/linux-gui-preview.sh [screenshot.png] [wait-seconds]
#   FILETERM_SKIP_BUILD=1  reuse the existing debug binary
#   FILETERM_KEEP_RUNNING=1  leave Xvfb and the app running (DISPLAY is printed)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$(realpath -m "${1:-$ROOT/fileterm-preview.png}")"
WAIT="${2:-15}"
DISPLAY_NUM="${FILETERM_DISPLAY:-:99}"
BIN="$ROOT/apps/tauri/src-tauri/target/debug/fileterm"

if [ "${FILETERM_SKIP_BUILD:-}" != "1" ] || [ ! -x "$BIN" ]; then
  (cd "$ROOT" && npm run build:packages)
  (cd "$ROOT/apps/tauri" && npx tauri build --debug --no-bundle)
fi

if ! xdpyinfo -display "$DISPLAY_NUM" >/dev/null 2>&1; then
  Xvfb "$DISPLAY_NUM" -screen 0 1600x1000x24 -nolisten tcp >/tmp/fileterm-xvfb.log 2>&1 &
  for _ in $(seq 50); do xdpyinfo -display "$DISPLAY_NUM" >/dev/null 2>&1 && break; sleep 0.1; done
fi
export DISPLAY="$DISPLAY_NUM"
# WebKitGTK's GPU/DMABUF paths fail without a real GPU; force software rendering.
export WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1

dbus-launch --exit-with-session "$BIN" >/tmp/fileterm-app.log 2>&1 &
APP_PID=$!
sleep "$WAIT"
import -display "$DISPLAY_NUM" -window root "$OUT"
echo "Screenshot: $OUT (app log: /tmp/fileterm-app.log)"

if [ "${FILETERM_KEEP_RUNNING:-}" = "1" ]; then
  echo "App still running on DISPLAY=$DISPLAY_NUM (pid $APP_PID)"
else
  kill "$APP_PID" 2>/dev/null || true
  pkill -f "$BIN" 2>/dev/null || true
fi
