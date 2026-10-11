# AGENTS.md - Zeebo core for Chimera

This repository builds Zeebx (https://github.com/ZeebxTeam/zeebx-emu), a
Zeebo emulator written in Rust, as a core for Chimera
(https://github.com/ToolAssisted-run/chimera), a frontend for tool-assisted
speedruns. It produces one file, `zeebx.chimeraCore`: Zeebx compiled to run
inside Chimera's sandbox (miniBox), a thin layer that runs the console one
frame at a time, and the declarations Chimera reads (controllers, settings,
file types). Zeebx is a git submodule at a fixed commit; every change to it
is a numbered patch. The picture is drawn by Zeebx's own software renderer.

Read [docs/BUILDING.md](docs/BUILDING.md) before building and
[docs/PLAN.md](docs/PLAN.md) before changing how the core works: it is the
design log, with what was decided, why, and what was measured.

## Layout

- `extern/zeebx` - Zeebx, a git submodule at a fixed commit.
- `patches/` - the numbered patches applied to `extern/zeebx`.
- `waterbox/guest/` - the core: the functions Chimera's engine calls
  (`src/lib.rs`), built for the sandbox and for the host from one crate.
  `x86_64-unknown-waterbox.json` is the sandbox's build target.
- `waterbox/driver/` - the in-memory filesystem everything the console
  keeps lives in (`memfs.cc`, shared with the Vita3K and Xenia cores).
- `waterbox/run-native/` - the native runner. `waterbox/run-wbx.c` - the
  sandbox runner. They take the same options and print the same lines.
- `waterbox/*.sh` - the builds and the test script (`run-gate.sh`).
  `make-patch.py` writes a patch from the tree.
- `waterbox/waterbox.config`, `file_slots.json`, `default_keybinds.json`,
  `package-licenses.json` - what the package declares. Written by hand.
- `.github/workflows/chimera.yml` - CI: build, test, publish.
- `build/` - every build output. `tests/roms/` - test games. Both ignored by
  git.

## Build and test

```sh
git submodule update --init
waterbox/build-native.sh
waterbox/build-package.sh -r <chimera>
CHIMERA_ROOT=<chimera> waterbox/run-gate.sh
```

The test script must end with `0 failed`. Run it before every commit that
changes the core, and say in the commit what it printed.

Most of its tests need a Zeebo game, which this repository cannot contain.
Without one they are skipped, and a skipped test proves nothing: a change to
how the console runs must be tested where the games are. If you do not have
them, say so in your report.

## Rules

- **Never edit `patches/` by hand.** Edit the tree under `extern/zeebx`, then
  run `python3 waterbox/make-patch.py <NNNN-name>`.
  `waterbox/apply-patches.sh` must then say "already applied".
- **The native run is the reference.** `run-native` and `run-wbx` must print
  the same lines for the same input. A change that makes them differ is
  wrong even when both look right.
- **The same input gives the same run, every time.** Nothing may read the
  host's clock, start a thread whose timing matters, or read a file outside
  the in-memory filesystem. A frame is 1/60 of a second of the game's own
  clock.
- **A new test is seen to fail once.** Break the thing it checks, on
  purpose, and see the test say so before trusting it.
- **No game and nothing of the console's software is committed** or uploaded
  to CI.
- **Changing what a setting means, a button's place, or the length of a
  frame changes every movie made with it.** That is the owner's decision.
- **Write in plain language.** Commits, documents and comments are read by
  people who have never seen this repository. `.md` files are ASCII.
- **Commits**: `type(scope): what changed`, a short body saying what
  changed, why, and how it was tested with the numbers. Stage paths by name.
  No amending, no force push.

## Moving to a newer Zeebx

Zeebx changes daily. To move: check out the new commit in `extern/zeebx`,
run `waterbox/apply-patches.sh`, mend the patches that no longer apply, and
run the whole test script with the games. A newer Zeebx is another machine:
a movie made with the old one may not play back.

## Publishing

CI publishes the package: a `dev` release on every push to `main` whose
tests pass, and a dated `nightly` when `main` moved. A package built by hand
is stamped `<commit>+local` and is for testing only.
