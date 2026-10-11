#!/bin/sh
# The core's test script. Each line says what was compared. It must end with
# "0 failed", and a SKIP is not a pass.
#
#   waterbox/run-gate.sh
#
# Needs waterbox/build-native.sh and waterbox/build-guest.sh (or
# build-package.sh) to have run. CHIMERA_ROOT=<a Chimera checkout> adds the
# tests that run the package in Chimera's headless runner.
#
# The tests with a game need files this repository cannot contain:
#   tests/roms/279036.7z (Um Jogo de Ovos), or ZEEBX_TEST_GAME=<a game>
#   tests/roms/276153.7z (Quake II), or ZEEBX_TEST_GAME_3D=<a 3D game>
# Without them only Zeebx's own unit tests run.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/.." && pwd)"
native="$root/build/native/release/run-native"
wbx="$root/build/wbx"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
ok=0; failed=0; skipped=0
report() { # <test> <PASS|FAIL|SKIP> <what>
	printf '%-22s %-6s %s\n' "$1" "$2" "$3"
	case "$2" in PASS) ok=$((ok + 1)) ;; FAIL) failed=$((failed + 1)) ;; *) skipped=$((skipped + 1)) ;; esac
}
# what a run ended with
last() { tail -1 "$1" 2> /dev/null | cut -c1-110; }
sandbox() { "$wbx/run-wbx" "$wbx/core.wbx" "$@"; }

game="${ZEEBX_TEST_GAME:-$root/tests/roms/279036.7z}"
game3d="${ZEEBX_TEST_GAME_3D:-$root/tests/roms/276153.7z}"
frames="${ZEEBX_GATE_FRAMES:-900}"
half=$((frames / 2))
# button 1 of the first controller (the fifth button the package declares),
# held for six frames a little before half way
press="--press $((half - 50)):6:10"
have_native=0; have_sandbox=0
[ -x "$native" ] && [ -f "$game" ] && have_native=1
[ -x "$wbx/run-wbx" ] && [ -f "$wbx/core.wbx" ] && [ -f "$game" ] && have_sandbox=1
why_native="no build/native/release/run-native: run waterbox/build-native.sh"
why_sandbox="no build/wbx/core.wbx: run waterbox/build-guest.sh"
[ -f "$game" ] || why_native="needs tests/roms/279036.7z, or ZEEBX_TEST_GAME=<a game>"
[ -f "$game" ] || why_sandbox="$why_native"

# Zeebx's own unit tests, built for this computer. They need no game.
if ! command -v cargo > /dev/null 2>&1; then
	report "upstream:tests" SKIP "no cargo on this computer"
else
	( cd "$root/extern/zeebx" && CARGO_TARGET_DIR="$root/build/upstream-tests" cargo +nightly test --release --no-default-features --lib ) > "$work/unit.out" 2>&1
	result="$(grep '^test result:' "$work/unit.out" | tail -1)"
	if echo "$result" | grep -q '^test result: ok\. [1-9][0-9]* passed; 0 failed'; then
		report "upstream:tests" PASS "Zeebx's unit tests: $(echo "$result" | sed 's/^test result: ok\. //; s/; 0 measured.*//')"
	else
		report "upstream:tests" FAIL "Zeebx's unit tests: ${result:-they did not build ($(grep -m1 '^error' "$work/unit.out"))}"
	fi
fi

# Two native runs of a game print the same lines, and a run with a button
# pressed prints others. The second half is the control: without it, "the
# same" could be two runs in which nothing happened.
if [ "$have_native" = 0 ]; then
	report "game:repeats" SKIP "$why_native"
	report "input:pad" SKIP "$why_native"
else
	"$native" "$game" --frames "$frames" --every 60 > "$work/a.out" 2> "$work/a.err"
	"$native" "$game" --frames "$frames" --every 60 > "$work/b.out" 2> "$work/b.err"
	"$native" "$game" --frames "$frames" --every 60 $press > "$work/p.out" 2> "$work/p.err"
	if [ -s "$work/a.out" ] && cmp -s "$work/a.out" "$work/b.out" && grep -q "^frame=$frames " "$work/a.out"; then
		report "game:repeats" PASS "$frames frames, twice, the same lines: $(last "$work/a.out")"
	else
		report "game:repeats" FAIL "first [$(last "$work/a.out")] second [$(last "$work/b.out")] $(tail -1 "$work/a.err")"
	fi
	if [ -s "$work/p.out" ] && ! cmp -s "$work/a.out" "$work/p.out"; then
		report "input:pad" PASS "button 1 held for six frames from frame $((half - 50)) makes another run: $(last "$work/p.out")"
	else
		report "input:pad" FAIL "the run with the button pressed is the run without it"
	fi
fi

# The sandboxed core prints the native build's lines: the same clock, the same
# number of instructions, the same picture and sound, every sixtieth frame.
if [ "$have_sandbox" = 0 ]; then
	report "game:sandbox" SKIP "$why_sandbox"
elif [ "$have_native" = 0 ]; then
	report "game:sandbox" SKIP "no native run to compare with"
else
	sandbox "$game" --frames "$frames" --every 60 $press > "$work/w.out" 2> "$work/w.err"
	if cmp -s "$work/p.out" "$work/w.out"; then
		report "game:sandbox" PASS "$frames frames with the button press: the sandbox prints the native build's lines"
	else
		report "game:sandbox" FAIL "native [$(last "$work/p.out")] sandbox [$(last "$work/w.out")] $(tail -1 "$work/w.err")"
	fi
fi

# The same for a game drawn in 3D by the software renderer.
if [ ! -f "$game3d" ]; then
	report "game:3d" SKIP "needs tests/roms/276153.7z, or ZEEBX_TEST_GAME_3D=<a 3D game>"
elif [ ! -x "$native" ] || [ ! -x "$wbx/run-wbx" ] || [ ! -f "$wbx/core.wbx" ]; then
	report "game:3d" SKIP "needs both builds (waterbox/build-native.sh, waterbox/build-guest.sh)"
else
	"$native" "$game3d" --frames "$frames" --every 60 > "$work/n3.out" 2> "$work/n3.err"
	sandbox "$game3d" --frames "$frames" --every 60 > "$work/w3.out" 2> "$work/w3.err"
	if [ -s "$work/n3.out" ] && cmp -s "$work/n3.out" "$work/w3.out" && grep -q "^frame=$frames " "$work/n3.out"; then
		report "game:3d" PASS "$frames frames of a 3D game: the sandbox prints the native build's lines: $(last "$work/n3.out")"
	else
		report "game:3d" FAIL "native [$(last "$work/n3.out")] sandbox [$(last "$work/w3.out")] $(tail -1 "$work/w3.err")"
	fi
fi

# Savestates: one saved and loaded before every frame changes nothing, and
# one saved by a process and loaded by another carries on to the same end.
if [ "$have_sandbox" = 0 ] || [ "$have_native" = 0 ]; then
	why="$why_sandbox"; [ "$have_sandbox" = 1 ] && why="no native run to compare with"
	report "state:every-frame" SKIP "$why"
	report "state:other-process" SKIP "$why"
else
	sandbox "$game" --frames "$frames" --every 60 $press --rerecord > "$work/r.out" 2> "$work/r.err"
	if cmp -s "$work/p.out" "$work/r.out"; then
		report "state:every-frame" PASS "a savestate saved and loaded before each of $frames frames: the same lines"
	else
		report "state:every-frame" FAIL "unbroken [$(last "$work/p.out")] with states [$(last "$work/r.out")] $(tail -1 "$work/r.err")"
	fi
	sandbox "$game" --frames "$half" --every 60 $press --save-state "$work/half.state" --at "$half" > "$work/s1.out" 2> "$work/s1.err"
	sandbox "$game" --frames "$frames" --every 60 $press --state "$work/half.state" > "$work/s2.out" 2> "$work/s2.err"
	if [ -s "$work/s2.out" ] && [ "$(tail -1 "$work/s2.out")" = "$(tail -1 "$work/p.out")" ]; then
		report "state:other-process" PASS "saved after frame $half ($(wc -c < "$work/half.state") bytes), loaded by another process, it ends on the unbroken run's line"
	else
		report "state:other-process" FAIL "unbroken [$(last "$work/p.out")] carried over [$(last "$work/s2.out")] $(tail -1 "$work/s2.err")"
	fi
fi

# The package in Chimera's headless runner (chimera-run), from a checkout
# named by CHIMERA_ROOT that has the package installed by
# waterbox/build-package.sh -r <it>:
#  - a movie recorded there shows, at a frame, the picture run-wbx shows;
#  - a movie has columns for the controllers that are plugged in and no
#    others (a row is "||" and then one group ending in "|" a controller).
chimera_root="${CHIMERA_ROOT:-}"
engine="$chimera_root/build/meson-linux/chimera-run"
package="$chimera_root/build/Cores/zeebx.chimeraCore"
if [ -z "$chimera_root" ] || [ ! -x "$engine" ] || [ ! -f "$package" ]; then
	why="set CHIMERA_ROOT to a Chimera checkout with build/meson-linux/chimera-run and the package (waterbox/build-package.sh -r <it>)"
	report "frontend:picture" SKIP "$why"
	report "frontend:pads" SKIP "$why"
elif [ "$have_sandbox" = 0 ]; then
	report "frontend:picture" SKIP "$why_sandbox"
	report "frontend:pads" SKIP "$why_sandbox"
else
	fe_frames=600
	shot=$((fe_frames - 1))
	: > "$work/empty.movie"
	env -u DISPLAY "$engine" "$package" "$game" "$work/empty.movie" --frames "$fe_frames" --record "$work/fe.movie" --screenshot "$shot=$work/fe.tga" > "$work/fe.out" 2>&1
	ran=$?
	sandbox "$game" --frames "$shot" --every "$shot" --ppm "$work/fe.ppm" > /dev/null 2>&1
	verdict="$(python3 "$here/tests/same-picture.py" "$work/fe.tga" "$work/fe.ppm" 2>&1)"
	same_rc=$?
	if [ "$ran" = 0 ] && grep -q "^frames=$fe_frames" "$work/fe.out" && [ "$same_rc" = 0 ]; then
		report "frontend:picture" PASS "$fe_frames frames in chimera-run: its picture at frame $shot is run-wbx's ($verdict)"
	else
		report "frontend:picture" FAIL "chimera-run exit $ran [$(tail -1 "$work/fe.out")]; the pictures: $verdict"
	fi
	bars() { # <how many controllers>: the bars in a recorded row
		env -u DISPLAY "$engine" "$package" "$game" "$work/empty.movie" --frames 3 --record "$work/pads$1.movie" \
			--settings "{\"pads\":\"$1\"}" > "$work/pads$1.out" 2>&1
		head -1 "$work/pads$1.movie" 2> /dev/null | tr -cd '|' | wc -c
	}
	one="$(bars 1)"; two="$(bars 2)"
	if [ "$one" = 3 ] && [ "$two" = 4 ]; then
		report "frontend:pads" PASS "a recorded movie has one controller's columns with one plugged in and two with two"
	else
		report "frontend:pads" FAIL "bars in a row: $one with one controller (3 expected), $two with two (4)"
	fi
fi

echo
echo "$ok ok, $failed failed, $skipped skipped"
[ "$failed" -eq 0 ]
