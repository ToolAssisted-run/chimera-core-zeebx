# Zeebo core - plan and design log

## What this is

A Chimera core for the Zeebo, the console TecToy sold in Brazil and Mexico
from 2009, built on Zeebx (https://github.com/ZeebxTeam/zeebx-emu). Requested
in chimera issue 240. The owner, 2026-10-11: "Let's give Zeebo a try first".

Where it stands (2026-10-11): both of the owner's test games run in
Chimera's sandbox a frame at a time and give exactly what the native build
gives - the same clock, instruction count, picture and sound at every checked
frame - including with a savestate saved and loaded before every frame, and
with a savestate carried into another process. There is no package for
Chimera yet (no controller, settings or file declarations), no test script,
and nothing is published.

## The version it builds

Zeebx v0.4.2, commit 880e46e (2026-10-11), as the submodule `extern/zeebx`.
The project is one month old and changes daily; its default branch is
`development` and releases are cut from `master`. A release tag was chosen
so that the version has a name.

## The questions a new core has to answer (chimera docs/new-core.md)

Answered from the source, not from the website.

- **Is it a new core?** Yes. No core here runs the Zeebo's system.
- **May it be distributed and modified?** Yes. The source is
  GPL-2.0-or-later. Upstream distributes its binaries under GPLv3 because
  they link Apache-2.0 libraries, and names every library in
  `THIRD-PARTY-NOTICES.txt`.
- **Can it be one thread on one clock?** It already is. A Zeebo game never
  touches hardware: it calls the console's system (Qualcomm BREW 4.0.2)
  through tables of function pointers, and Zeebx answers those calls itself.
  The game's ARM code runs on one thread, and the clock the game sees is
  counted from the instructions that ran. Time the game spends waiting is
  skipped, not waited for.
- **Where does the picture come from?** From Zeebx's own software renderer
  for the console's 3D (OpenGL ES 1.1) and its 2D screen: 640x480, 16 bits a
  pixel. No graphics card is needed. Upstream also has a renderer for a
  graphics card; this core does not use it.
- **Does it recompile?** Yes, with Dynarmic, the recompiler the touchHLE
  core already runs in the sandbox. Memory goes through a page table and
  function calls, not through faults. There is an interpreter as well.
- **What must the user bring?** The game, as upstream reads it: a `.7z` or
  `.zip` with `mif/` and `mod/` folders, or a `.mod`. No system software.
  Optional: the console's system font (`tectoy.ttf`, which ships inside the
  console's menu package, Z-Wheel) for games that draw text without bringing
  a font, and a sound bank (`.sf2`) for MIDI music.
- **What can a test run on a public runner?** Zeebx's own unit tests (608
  pass). No game may be put on a runner. A small test program for the
  console's system could be written later, as the touchHLE core did.
- **How big is it?** Small. The largest test game is 74 MB unpacked.

## What has to change for a TAS

Found by reading; none of it is done yet.

- The date the game sees starts from the host's clock
  (`src/machine/mod.rs`, `epoch_seconds`). It must be a setting.
- The software renderer splits the picture into bands, one per host
  processor, each on its own thread. Upstream says the picture is the same
  whatever the number. In the sandbox it must be one.
- `Session::run_frame` ends when the game presents a picture or after 16 ms
  of the game's clock, whichever comes first, so a frame has no fixed
  length. A Chimera frame must be a fixed slice of the game's clock.
- A game is unpacked to a folder on disk before it runs, and what it saves
  goes to another. In the sandbox both must be in memory. Upstream notes
  that a run can differ depending on whether the game was already unpacked
  (760,815 instructions against 760,921), cause unknown; in the sandbox it
  is always unpacked fresh.
- Upstream's rewind runs a helper thread. The core does not use it:
  Chimera's savestates are the sandbox's own.

## First measurements (2026-10-11)

Native build, software renderer, `waterbox/run-native`:

| game | frames | game clock | host time | where it is |
| --- | --- | --- | --- | --- |
| 279036 (title not known yet) | 600 | 11.2 s | 1.1 s | main menu |
| 276153, Quake II | 900 | 15.3 s | 1.8 s | main menu |

Two runs of 279036 from fresh work folders gave the same clock, instruction
count, picture digest and sound digest at frames 300 and 600.

Upstream's unit tests do not compile at v0.4.2: one test in the interface
code (`src/ui/navegacao.rs`, line 139) passes an array of 6 where the
function now takes 8. With that line fixed in a scratch copy: 608 passed,
4 ignored.

## Building for the sandbox: what the first attempts turned up

- The build target is named `x86_64-unknown-waterbox`. With the touchHLE
  core's name for it (`waterbox-guest`) the hashing library blake3 did not
  recognise an x86-64 machine: its build script reads the processor from
  the target's name.
- The `dynarmic` crate's build script builds Dynarmic's own test programs,
  and a program cannot be linked for the sandbox that way. It has no switch
  for them, so `waterbox/guest-toolchain.cmake` turns them off.

With those two, the whole emulator and its libraries compile for the
sandbox (65 s on 20 threads). Linking `core.wbx` is the next step.

## The core, natively and in the sandbox (2026-10-11)

`waterbox/guest` is the core: the functions Chimera's engine calls, over
Zeebx's `Session`. It is built twice - for the host, where
`waterbox/run-native` calls it, and for the sandbox, as `core.wbx`, which
`waterbox/run-wbx` runs through miniBox. Both runners take the same options
and print the same lines, so their outputs are compared with `cmp`.

What was decided:

- **A frame is 1/60 of a second of the game's clock.** Frame n ends when the
  clock reaches n x 1000 / 60 milliseconds (patch 0001,
  `Session::run_until_ms`). The clock can jump past that when the game is
  only waiting; then the next frames run nothing until their time comes.
  Every frame carries the same amount of sound: 735 stereo samples at
  44.1 kHz.
- **The date is a setting** (`rtc_start`, default 2010-01-01), given to the
  machine before it starts (patch 0001).
- **Files are in memory.** The in-memory filesystem written for the Vita3K
  core and used by the Xenia core (`waterbox/driver/memfs.cc`) sits under
  the C library in both builds. The game's package is read through it, and
  everything Zeebx unpacks or a game saves lives in it, dated the moment
  the console was switched on. One change was needed: Rust's standard
  library asks whether `statx` exists by calling it with null pointers, and
  the filesystem's version crashed on that.
- **The picture is drawn on one thread in the sandbox.** miniBox reports one
  processor, so Zeebx's renderer uses one band and starts no thread. The
  native build uses as many as the host has; the pictures are the same.
- **The controller**: 14 buttons (Up, Down, Left, Right, 1, 2, 3, 4, ZL, ZR,
  Start, Back, and the two stick presses) and 4 axes (two sticks, -128 to
  127), for each of two ports.

What was measured, sandbox against native, lines printed every 60 frames:

| run | result |
| --- | --- |
| 279036, 900 frames, button 1 held for 6 frames from frame 400 | same lines |
| the same with a savestate saved and loaded before every frame | same lines |
| the same, saved after frame 450 (23.7 MB) and finished by another process | ends on the same line |
| Quake II, 900 frames | same lines |

A press changes the run: without it the instruction count at frame 900 is
586,516,928, with it 586,839,418, and the picture differs.

Host time for 900 frames (15 s of game clock): 1.5 to 2 s natively, 9.4 s
(279036) and 15.2 s (Quake II) in the sandbox. Where the sandbox's extra
time goes has not been measured yet.

Building `core.wbx` turned up one more thing: the `dynarmic` crate tells
cargo to find Dynarmic's libraries at link time, so they are not inside the
core's static library and `build-guest.sh` names them in the link.

## What is the owner's

- A public repository, a row in Chimera's list of official cores, and
  anything said on issue 240.
- The two test games are the owner's files, copied to `tests/roms/`, which
  git ignores.
