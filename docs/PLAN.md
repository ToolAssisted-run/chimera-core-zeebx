# Zeebo core - plan and design log

## What this is

A Chimera core for the Zeebo, the console TecToy sold in Brazil and Mexico
from 2009, built on Zeebx (https://github.com/ZeebxTeam/zeebx-emu). Requested
in chimera issue 240. The owner, 2026-10-11: "Let's give Zeebo a try first".

Where it stands (2026-10-11): both of the owner's test games run in
Chimera's sandbox a frame at a time and give exactly what the native build
gives - the same clock, instruction count, picture and sound at every checked
frame - including with a savestate saved and loaded before every frame, and
with a savestate carried into another process; all of the game's memory is
the same at the end as well. As a package
(`zeebx.chimeraCore`) both games run in Chimera's headless runner on Linux
and on Windows, with the same pictures. There is no test script yet,
Chimera's window has not been tried, and nothing is published.

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
| 279036, Um Jogo de Ovos | 600 | 11.2 s | 1.1 s | main menu |
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

## The package (2026-10-11)

`waterbox/build-package.sh -r <chimera checkout>` builds
`zeebx.chimeraCore`: the core, what it declares (`waterbox.config`,
`file_slots.json`, `default_keybinds.json`) and the licence texts.

- System id `ZEEBO`, named "Zeebo". A project is one game: a `.7z`, a
  `.zip` or a `.mod`.
- Two settings: Controllers (1 or 2) and Clock at Power-On.
- The built core is under the GPL version 3, as Zeebx's own binaries are.
  The package carries Zeebx's licence, the GPL 3 text and Zeebx's own list
  of third-party notices, which names Dynarmic, SQLite and every Rust
  library with its licence. That list was written for Zeebx's desktop
  program, so it also names libraries this core does not link.
- Memory for RAM tools came the same day: see "The game's memory" below.

What was measured:

- Chimera's package checks (`InstalledCorePackagesTests`,
  `MnemonicUniquenessTests`): 17 of 17.
- `chimera-run` on Linux: 279036 for 600 frames, recording a movie (one
  controller's columns: 4 axes and 14 buttons). Its picture matched
  `run-wbx`'s - but on a still screen, which proved less than it seemed:
  see "A test that passed on a still picture" below.
- `chimera-run.exe` on Windows: 279036 for 600 frames and Quake II for 900.
  The pictures at frames 599 and 899 are the ones Linux gives.

Two things the first runs in `chimera-run` turned up: the engine mounts the
game under a name the core has to look for (its own name, the full path, or
"rom"), and it requires all five memory-domain functions even when there
are no domains.

## The test script (2026-10-11)

`waterbox/run-gate.sh` runs nine tests, about three minutes with both games
and `CHIMERA_ROOT` set. Each line says what was compared, and the script
ends with how many passed, failed and were skipped.

- `upstream:tests`: Zeebx's own unit tests (608 pass, 4 ignored). They need
  no game, so they are what a public CI runner can run. At v0.4.2 they do
  not compile as shipped: patch 0002 fixes the one stale line.
- `game:repeats`, `input:pad`: two native runs print the same lines, and a
  run with a button pressed prints others.
- `game:sandbox`, `game:3d`: the sandboxed core prints the native build's
  lines, for a 2D game and for a 3D one.
- `state:every-frame`, `state:other-process`: savestates.
- `frontend:picture`, `frontend:pads`: the package in `chimera-run`.

Seen to fail, by hand:

- a savestate that is one frame old, loaded half way
  (`RUN_WBX_BREAK_STALE_STATE=1`): from frame 480 on the clock is 33 ms
  behind and the lines differ;
- the runner's picture from another frame (560 instead of 599) against
  Chimera's: 307,033 of 307,200 pixels differ;
- `input:pad` and `game:repeats` check each other: one needs two runs to
  be the same, the other needs a run to differ.

Chimera's window was also run with the package on Windows, driven by a
script (`Chimera.exe --headless`): 600 frames of Um Jogo de Ovos (279036),
the project saved with one controller's columns, a clean exit.

## The game's memory, for RAM tools (2026-10-11)

RAM Watch and RAM Search get one bus, "Memory": the game's 32-bit address
space at the addresses the game itself uses (patch 0003 adds the read, the
write and the list of ranges to Zeebx's session). Most of that space is
empty, so the core also says where the game's writable memory is
(`GetBusRanges`): seven ranges, 82.9 MB in the two test games - the game's
own data, its heap, its stack, and the console's objects and surfaces. A
write goes through Zeebx's own path, which translates code again if it was
written over.

No memory domains are offered. The memory the recompiler runs on is a copy
Zeebx's processor owns, split into ranges, and a bus shows it at the game's
own addresses, which is what a person searching for a value wants.

Both runners now end with one line about the bus: how many ranges, how many
bytes, a digest of every byte read through the bus, and whether a byte that
is written reads back. So every comparison of the sandbox with the native
build now also compares all of the game's memory at the end of the run.
Measured on 279036 after 300 frames: 7 ranges, 82,945,164 bytes, the same
digest natively and in the sandbox, and a written byte reads back.

## Sound that differed from run to run (2026-10-11)

Playing Quake II into its first level (button 1 three times from the menu,
then walking, turning and firing) showed something the menus had not: the
sound of the native build and of the sandbox differed from the frame the
level's sounds began, while the clock, the picture and all of the game's
memory were the same. Two causes, found one after the other.

- **Two maths libraries.** Zeebx's sound code calls sin, pow and the like,
  which came from glibc natively and from musl in the sandbox, and the two
  round differently. Both builds now take them from one Rust library
  (`waterbox/guest/src/mathlib.rs`, as the touchHLE core does), and
  `build-guest.sh` refuses a core that still has musl's maths in it. This
  moved the first difference sixty frames later and did not remove it.
- **A hash map in the mixer.** Zeebx's mixer keeps the sounds that are
  playing in a hash map and adds them up in the order it walks the map. A
  hash map's order is random in every process, and floating-point addition
  rounds differently in another order. So two NATIVE runs of the same game
  did not make the same sound either, which is how it was found. Patch
  0004 makes those two maps sorted ones.

After both: two native runs print the same lines, and so does the sandbox,
over 3,600 frames of that play, sound included. The test `game:3d` now does
exactly this, so it is also the test of the sound.

The hash map is a defect in Zeebx itself and has not been reported to its
authors: that is the owner's to decide.

Speed, the same 3,600 frames (60 s of game clock): 15 s natively, 59 s in
the sandbox. In the level the sandbox runs at about real time on this
computer (20 threads, of which the sandbox uses one). The native build draws
the 3D picture on as many threads as the host has; the sandbox draws it on
one.

## Save data, out and in (2026-10-11)

What a game saves lives in the console's saves folder in memory, under a
folder named after the game's package. Um Jogo de Ovos writes a log there;
Quake II writes a copy of one of its own files and, once in a level, its
automatic save (`udata/save/current/game.ssv` and `server.ssv`).

- **Out**: Chimera's Export Save Data gets every file under that folder,
  named by its path.
- **In**: a project can hold a "Save data" file, the `.zip` that export
  writes. Its files are put back in the saves folder before the game
  starts. A name that would leave the folder is refused.

The test `frontend:savedata` exports what Quake II saved in its level
through `chimera-run`, changes one byte of one file, zips the result, and
starts a new run from it: at its first frame that file is there with the
changed byte, and without the save data it is not there at all. Whether the
game then offers the save in its Load Game menu has not been looked at.

## A test that passed on a still picture (2026-10-11)

`frontend:picture` first compared `chimera-run --screenshot 599` with
`run-wbx --frames 599` on Um Jogo de Ovos, and passed. It was wrong twice
over, and each error hid the other: `chimera-run` numbers its screenshots
from 0, so number 599 is the picture after 600 frames; and the game's menu
was standing still at that point, so frames 599 and 600 were the same
picture. It came out when a movie into Quake II's level was compared the
same way and 18,331 pixels differed.

The test now plays a movie into the level, compares screenshot N with the
picture after N+1 frames, and also requires that picture to differ from the
one two frames earlier (the game draws a new picture every second frame).
A still picture can no longer pass for a match.

The same day `state:other-process` was found comparing only the last line
of each run, which had become a line about saved files and would have been
the same for a run that had gone wrong. It now compares everything from the
last frame's line on. Seen to fail: a state saved after frame 449 and
labelled 450 ends 17 ms behind with other sound.

## What is the owner's

- A public repository, a row in Chimera's list of official cores, and
  anything said on issue 240.
- The two test games are the owner's files, copied to `tests/roms/`, which
  git ignores.
