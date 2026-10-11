#!/bin/bash
# Builds core.wbx: the Zeebo core for Chimera's sandbox (miniBox), and
# run-wbx, the program that runs it there the way Chimera's engine does.
#
#  1. cargo (nightly, -Z build-std) builds waterbox/guest for the sandbox's
#     target (guest/x86_64-unknown-waterbox.json: musl, large code model, no
#     red zone, no native thread-local storage). The C and C++ pieces - the
#     Dynarmic recompiler, SQLite, the in-memory filesystem - are built by
#     their crates' own build scripts, which are handed the sandbox's
#     compilers here through the cc and cmake crates' environment variables.
#  2. The resulting library is linked with miniBox's C++ runtime and the
#     shims in guest-syscalls.cpp into core.wbx, which miniBox's
#     check-wbx.sh then inspects.
#
# Usage: ./build-guest.sh -m <miniBox dir> [-j N]
#   (MINIBOX_DIR works too.) Needs: rustup nightly with rust-src, and a
#   miniBox checkout built with its C++ toolchain (build/meson-cpp) and its
#   host library (build/meson-linux).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/.." && pwd)"
minibox="${MINIBOX_DIR:-}"
jobs="$(nproc)"
while getopts "m:j:" opt; do
	case "$opt" in
		m) minibox="$OPTARG" ;;
		j) jobs="$OPTARG" ;;
		*) exit 2 ;;
	esac
done
[ -n "$minibox" ] || { echo "build-guest.sh: say where miniBox is (-m <dir> or MINIBOX_DIR)" >&2; exit 1; }
minibox="$(cd "$minibox" && pwd)"
mbc="$minibox/build/meson-cpp"
sr="$mbc/guest-sysroot"
[ -f "$sr/lib/libstdc++.a" ] || { echo "miniBox's C++ toolchain is missing at $sr" >&2; exit 1; }
gccver="$(basename "$(ls -d "$sr"/include/c++/* | head -1)")"

sh "$here/apply-patches.sh"

# The sandbox's compilers, for every build script in the tree. The cc crate's
# own default flags (it would add -fPIC) are switched off. thread_local is
# defined away: a sandboxed program has no thread pointer, and the console is
# one thread.
wb="-specs=$sr/lib/musl-gcc.specs -mcmodel=large -mstack-protector-guard=global -fno-stack-protector -fno-pic -fno-pie -fcf-protection=none -O2"
notls="-Dthread_local= -D_Thread_local= -D__thread="
export CRATE_CC_NO_DEFAULTS=1
export CC_x86_64_unknown_waterbox=gcc CXX_x86_64_unknown_waterbox=g++ AR_x86_64_unknown_waterbox=ar
export CFLAGS_x86_64_unknown_waterbox="$wb $notls"
export CXXFLAGS_x86_64_unknown_waterbox="$wb $notls -nostdinc++ -I$sr/include/c++/$gccver -I$sr/include/c++/$gccver/x86_64-linux-musl"
export CMAKE_TOOLCHAIN_FILE_x86_64_unknown_waterbox="$here/guest-toolchain.cmake"
export MINIBOX_DIR="$minibox"
export CMAKE_BUILD_PARALLEL_LEVEL="$jobs"
export CARGO_TARGET_DIR="$root/build/guest"

( cd "$here/guest" && cargo +nightly build --release -j "$jobs" )
lib="$root/build/guest/x86_64-unknown-waterbox/release/libzeebx_guest.a"
# Dynarmic and the libraries it is built with: the dynarmic crate names them
# to cargo as libraries to find at link time, so they are not inside the
# library above and are named here.
dyn="$(ls -td "$root"/build/guest/x86_64-unknown-waterbox/release/build/dynarmic/*/out/lib 2> /dev/null | head -1)"
[ -f "$dyn/libdynarmic.a" ] || { echo "build-guest.sh: no Dynarmic library under build/guest" >&2; exit 1; }

out="$root/build/wbx"
mkdir -p "$out"
g++ -c -specs "$sr/lib/musl-gcc.specs" -mcmodel=large -fno-pic -fno-pie -fno-stack-protector \
	-fcf-protection=none -O2 -nostdinc++ -I"$sr/include/c++/$gccver" \
	-I"$sr/include/c++/$gccver/x86_64-linux-musl" -o "$out/guest-syscalls.o" "$here/guest-syscalls.cpp"
gcc -c -specs "$sr/lib/musl-gcc.specs" -mcmodel=large -fno-pic -fno-pie -fno-stack-protector \
	-O2 -o "$out/libgcc-builtins.o" "$here/guest/libgcc-builtins.c"

# every function Chimera's engine may call, kept through the link
exports=""
for e in Init GetLoadError SetAxis FrameAdvance IsRunning GetFrameCount GetMachineTimeNs \
	GetExecutedTicks GetVideoBgra GetVideoWidth GetVideoHeight GetVsyncNumerator \
	GetVsyncDenominator GetAudio GetAudioSampleCount IsButtonActive IsAxisActive \
	GetSaveDataFileCount GetSaveDataFileName GetSaveDataFileSize GetSaveDataFileBuffer \
	GetMemoryDomainCount GetMemoryDomainName GetMemoryDomainPtr GetMemoryDomainSize \
	GetMemoryDomainWritable; do
	exports="$exports -Wl,-u,$e"
done

g++ -specs "$sr/lib/musl-gcc.specs" -mcmodel=large -fno-pic -fno-pie -fno-stack-protector \
	-static -no-pie -Wl,--eh-frame-hdr,-O2,--no-relax,-z,stack-size=8388608 \
	-T "$minibox/source/guest/linkscript.T" $exports \
	-o "$out/core.wbx" \
	"$mbc/source/guest/cxxglue.c.o" "$mbc/source/guest/emulibc.c.o" \
	"$out/guest-syscalls.o" "$out/libgcc-builtins.o" \
	-Wl,--start-group "$lib" "$dyn/libdynarmic.a" "$dyn/libfmt.a" "$dyn/libmcl.a" "$dyn/libZydis.a" -Wl,--end-group \
	-L"$sr/lib" -lstdc++ -lgcc -lgcc_eh -lc -lm
sh "$minibox/source/guest/check-wbx.sh" "$out/core.wbx"
echo "built: $out/core.wbx"

# run-wbx: runs core.wbx in the sandbox as the engine does (run-wbx.c)
mbhost="$minibox/build/meson-linux/source/host"
if [ -f "$here/run-wbx.c" ]; then
	gcc -O2 -g -Wall -I"$minibox/source/host" -o "$out/run-wbx" "$here/run-wbx.c" \
		"$mbhost/libminiboxhost.so" -Wl,-rpath,"$mbhost" -ldl
	echo "built: $out/run-wbx"
fi
