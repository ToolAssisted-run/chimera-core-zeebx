# Building the Zeebo core

This builds `zeebx.chimeraCore` from source and runs its tests. It has been
done on Ubuntu 24.04. A build from nothing takes about three minutes on a
20-thread machine.

## What you need

- A Chimera checkout, built (its README, "Building"), with miniBox's host
  library and C++ guest toolchain built too. `<chimera>` below is that
  checkout.
- Rust's nightly toolchain with its source, through rustup. The sandboxed
  build compiles Rust's standard library for its own target, which only
  the nightly toolchain does:

  ```sh
  rustup toolchain install nightly --profile minimal -c rust-src
  ```

- A C++ compiler, cmake and ninja, for the Dynarmic recompiler.

```sh
sudo apt-get install -y --no-install-recommends meson ninja-build \
  build-essential cmake pkg-config python3
```

miniBox, in the Chimera checkout:

```sh
mb=<chimera>/extern/chimera-common-minibox
meson setup "$mb/build/meson-linux" "$mb" && meson compile -C "$mb/build/meson-linux"
meson setup "$mb/build/meson-cpp" "$mb" -Dguest_cpp=true && meson compile -C "$mb/build/meson-cpp"
```

## The sources

```sh
git submodule update --init
```

`extern/zeebx` is Zeebx at a fixed commit. It is never edited by hand for
keeps: every change is a numbered patch in `patches/`, applied by
`waterbox/apply-patches.sh` (the build scripts call it). To change Zeebx,
edit the tree under `extern/zeebx`, then run
`python3 waterbox/make-patch.py <NNNN-name>`, which writes what the tree
changes beyond the patches that already exist as a new patch.

## The steps

```sh
waterbox/build-native.sh                 # the core for this computer, and run-native
waterbox/build-package.sh -r <chimera>   # the sandboxed core, run-wbx, and the package
```

- `build/native/release/run-native` runs a game on the native build.
- `build/wbx/core.wbx` is the sandboxed core and `build/wbx/run-wbx` runs it
  through miniBox, the way Chimera does.
- The package is installed as `<chimera>/build/Cores/zeebx.chimeraCore`.

The two runners take the same options and print the same lines - the game's
clock, the number of ARM instructions run, and digests of the picture and of
the sound - so their outputs are compared with `cmp`:

```sh
build/native/release/run-native <game> --frames 900 --every 60 > native.out
build/wbx/run-wbx build/wbx/core.wbx <game> --frames 900 --every 60 > sandbox.out
cmp native.out sandbox.out
```

`--press F:N:MASK` holds buttons (a hexadecimal mask, one bit a button in
the order the package declares them) for N frames from frame F. `--ppm FILE`
writes the last picture. `run-wbx` also takes `--rerecord` (save and load a
savestate before every frame) and `--save-state FILE --at FRAME` with
`--state FILE` (carry a savestate into another process).

## The tests

```sh
CHIMERA_ROOT=<chimera> waterbox/run-gate.sh
```

Each line says what was compared. The script ends with a count of tests
that passed, failed and were skipped, and exits with an error if any failed.
A skipped test is not a pass.

Zeebx's own unit tests need no game. The other tests need files this
repository cannot contain:

- `tests/roms/279036.7z` (Um Jogo de Ovos), or `ZEEBX_TEST_GAME=<a game>`;
- `tests/roms/276153.7z` (Quake II), or `ZEEBX_TEST_GAME_3D=<a 3D game>`.

With them: two native runs are the same and a button press makes another;
the sandboxed core prints the native build's lines, for a 2D game and a 3D
one; a savestate saved and loaded before every frame changes nothing, and
one carried into another process ends in the same place; and, with
`CHIMERA_ROOT` set, the package runs in Chimera's headless runner with the
same picture, and a movie has columns only for the controllers plugged in.

Chimera's own checks of an installed package:

```sh
cd <chimera>
CHIMERA_CORES_DIR=$PWD/build/Cores build/meson-linux/source/ui/test-packages
```

## What CI does

`.github/workflows/chimera.yml` builds everything above from a fresh clone
on a public runner, runs the test script and Chimera's package checks, and
publishes the package. A runner has no game, so there only Zeebx's unit
tests run; the tests with a game are run where the games are, before
anything is pushed.

## When something goes wrong

- "miniBox's C++ toolchain is missing": miniBox's `build/meson-cpp` is not
  built.
- `apply-patches.sh` says the tree is "partly patched": a file under
  `extern/zeebx` was edited and no patch has it yet. Write the patch, or put
  the tree back with the command the message names.
- The sandboxed core stops with "asked for system call N": miniBox ends a
  program that makes a system call it does not know. The answer usually
  belongs in `waterbox/guest-syscalls.cpp` or `waterbox/driver/memfs.cc`.
- A library's build script does not recognise the target: the target file
  is named `x86_64-unknown-waterbox.json` because some build scripts read
  the processor from the target's name.
