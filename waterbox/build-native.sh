#!/bin/sh
# Builds run-native: the core compiled for this computer, with the runner that
# calls it the way Chimera's engine does. What it prints is what the sandboxed
# core is compared with.
#
#   waterbox/build-native.sh
#
# Needs: rustup's nightly toolchain, a C++ compiler, cmake and ninja (for the
# Dynarmic recompiler).
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/.." && pwd)"
sh "$here/apply-patches.sh"
( cd "$here/run-native" && CARGO_TARGET_DIR="$root/build/native" cargo +nightly build --release )
echo "built: build/native/release/run-native"
