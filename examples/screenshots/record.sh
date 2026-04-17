#!/bin/bash
# Record a macOS Terminal.app window rendering `mat <demo.md>`, auto-scrolling
# from top to bottom of the output, then convert the MOV to a palette-optimized
# GIF. Produces deterministic, inspectable terminal recordings without any
# manual interaction — drive this from CI or a release script.
#
# Usage: record.sh <demo-file.md> <output.gif>
set -eu

demo="$1"
out_gif="$2"
repo="$(cd "$(dirname "$0")/../.." && pwd)"

: "${MAT_WIDTH:=110}"
: "${WINDOW_W:=1250}"
: "${WINDOW_H:=820}"
: "${GIF_WIDTH:=900}"
: "${GIF_FPS:=10}"
: "${LINE_DELAY:=0.15}"       # seconds between each one-line scroll
: "${HEAD_HOLD:=1.8}"         # static hold on the top before scrolling
: "${TAIL_HOLD:=1.8}"         # static hold at the bottom before stopping
: "${EXTRA_TAIL:=1.0}"        # safety margin for recorder to flush

lines_total=$(./target/release/mat --force-color --width "$MAT_WIDTH" "$demo" | wc -l | tr -d ' ')
# Terminal.app shows ~38-42 rows at this bound; scroll 1 line per tick.
scroll_ticks=$((lines_total + 8))
scroll_time=$(awk "BEGIN { print $scroll_ticks * $LINE_DELAY }")
rec_seconds=$(awk "BEGIN { printf \"%.0f\", $HEAD_HOLD + $scroll_time + $TAIL_HOLD + $EXTRA_TAIL }")

echo "→ $demo — $lines_total lines, scrolling for ${scroll_time}s, recording ${rec_seconds}s"

# Drop a temp wrapper into a short path so the shell-prompt echo at the top of
# the recording reads `bash /tmp/mat-play.sh ...` instead of the full unset +
# clear + absolute-path mat invocation. Wrapper does the env scrub (so mat
# falls back to half-block images — Terminal.app has no Kitty/iTerm2/Sixel
# support), clears scrollback with `\e[3J`, then execs mat.
wrap=/tmp/mat-play.sh
cat >"$wrap" <<'WRAP'
#!/bin/bash
unset TERM_PROGRAM KITTY_WINDOW_ID GHOSTTY_RESOURCES_DIR ITERM_SESSION_ID VSCODE_INJECTION WEZTERM_EXECUTABLE
printf '\033c\033[3J'
exec "$1" --force-color --width "$2" "$3"
WRAP
chmod +x "$wrap"

# Short command that shows up as the prompt echo in the recording.
cmd="cd '$repo' && clear && $wrap ./target/release/mat $MAT_WIDTH '$demo'"

# Close every existing Terminal window so our recording target is
# unambiguous — otherwise `tell process "Terminal"` routes keystrokes to
# whatever window happens to be frontmost, which is often a leftover
# probe window and ruins the capture.
osascript <<APP
tell application "Terminal"
  activate
  try
    close every window saving no
  end try
  delay 0.3
end tell
APP

# Open Terminal.app, size the window, run mat, return the window ID.
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

# Ensure the Terminal process is frontmost so AXUIElement keystrokes
# from System Events hit it and not, say, the IDE window that launched
# this script.
osascript <<APP
tell application "Terminal" to activate
delay 0.4
APP

# Snap scrollback to the top before recording starts.
osascript <<APP
tell application "System Events"
  tell process "Terminal"
    key code 115 using {command down}
  end tell
end tell
APP

sleep 0.4

mov="${out_gif%.gif}.mov"
rm -f "$mov"

# Start recording in the background — screencapture -V blocks for the duration.
screencapture -V "$rec_seconds" -l "$win_id" -x "$mov" &
cap_pid=$!

# Brief static hold so viewers see the top of the document.
sleep "$HEAD_HOLD"

# Line-by-line scroll via Cmd+Down, once per LINE_DELAY seconds.
osascript <<APP
tell application "System Events"
  tell process "Terminal"
    repeat $scroll_ticks times
      key code 125 using {command down}
      delay $LINE_DELAY
    end repeat
  end tell
end tell
APP

# Hold the bottom briefly so the last frame is the tail of the doc.
sleep "$TAIL_HOLD"

# Wait for screencapture to complete (it stops on its own after rec_seconds).
wait "$cap_pid" || true

# Close the Terminal window cleanly.
osascript <<APP || true
tell application "Terminal"
  close (first window whose id is $win_id) saving no
end tell
APP

if [ ! -s "$mov" ]; then
  echo "  ✗ no video produced"
  exit 1
fi

# MOV → palette-optimized GIF at a sane width, preserving the aspect ratio.
ffmpeg -y -v error -i "$mov" \
  -vf "fps=$GIF_FPS,scale=$GIF_WIDTH:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=256[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5" \
  "$out_gif"

mov_bytes=$(stat -f %z "$mov" 2>/dev/null || echo 0)
gif_bytes=$(stat -f %z "$out_gif" 2>/dev/null || echo 0)
echo "  ✓ $out_gif (mov: $mov_bytes B, gif: $gif_bytes B)"

# Keep the GIF, drop the intermediate MOV.
rm -f "$mov"
