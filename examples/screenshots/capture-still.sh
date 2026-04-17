#!/bin/bash
# Capture a single high-resolution screenshot of macOS Terminal.app rendering
# `mat <demo-file.md>` and save it as a PNG. Captures at Retina native
# resolution via `screencapture -l <winId>` so the source pixels roughly
# match the iTerm2-protocol display size when mat embeds the image inline
# in README.md — preserves text legibility inside the screenshot.
#
# Usage: capture-still.sh <demo-file.md> <output.png>
set -eu

demo="$1"
out_png="$2"
repo="$(cd "$(dirname "$0")/../.." && pwd)"

: "${MAT_WIDTH:=110}"
: "${WINDOW_W:=1250}"
: "${WINDOW_H:=820}"
: "${TARGET_W:=1800}"     # final downsample width in pixels

# Wrapper keeps Terminal.app's visible prompt line short and scrubs env vars
# so mat falls back to half-block (Terminal.app has no Kitty/iTerm2/Sixel).
wrap=/tmp/mat-play-still.sh
cat >"$wrap" <<'WRAP'
#!/bin/bash
unset TERM_PROGRAM KITTY_WINDOW_ID GHOSTTY_RESOURCES_DIR ITERM_SESSION_ID VSCODE_INJECTION WEZTERM_EXECUTABLE
printf '\033c\033[3J'
exec "$1" --force-color --width "$2" "$3"
WRAP
chmod +x "$wrap"

cmd="cd '$repo' && clear && $wrap ./target/release/mat $MAT_WIDTH '$demo'"

# Close every existing Terminal window so the recording target is unambiguous.
osascript <<APP >/dev/null
tell application "Terminal"
  activate
  try
    close every window saving no
  end try
  delay 0.3
end tell
APP

win_id=$(osascript <<APP
tell application "Terminal"
  activate
  set newTab to do script "$cmd"
  delay 0.5
  set newWin to the window 1
  set bounds of newWin to {80, 60, 80 + $WINDOW_W, 60 + $WINDOW_H}
  set frontmost of newWin to true
  set custom title of newTab to "mat $(basename "$demo")"
  delay 1.5
  return id of newWin
end tell
APP
)

# Activate + Cmd+Home so the snapshot shows the top of mat's output
# (not whatever scrolled to the bottom as the demo finished).
osascript <<APP >/dev/null
tell application "Terminal" to activate
delay 0.3
tell application "System Events"
  tell process "Terminal"
    key code 115 using {command down}
  end tell
end tell
delay 0.6
APP

# Capture the window at native Retina resolution.
raw="${out_png%.png}.retina.png"
rm -f "$raw"
screencapture -l "$win_id" -x -o "$raw"

osascript <<APP >/dev/null || true
tell application "Terminal"
  close (first window whose id is $win_id) saving no
end tell
APP

if [ ! -s "$raw" ]; then
  echo "  ✗ no PNG produced"
  exit 1
fi

# Downsample to TARGET_W width preserving aspect. Keeps repo weight sane
# while leaving the image high-res enough that text inside stays legible
# when iTerm2 displays it at full terminal width.
ffmpeg -y -v error -i "$raw" \
  -vf "scale=$TARGET_W:-1:flags=lanczos" \
  "$out_png"

rm -f "$raw"
raw_was=$(stat -f %z "$out_png" 2>/dev/null || echo 0)
echo "  ✓ $out_png (${raw_was} B)"
