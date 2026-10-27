# chimera-core-zeebx

A Zeebo core for [Chimera](https://github.com/ToolAssisted-run/chimera), the
frontend for tool-assisted speedruns. The Zeebo is the console TecToy sold in
Brazil and Mexico from 2009. The core is built on
[Zeebx](https://github.com/ZeebxTeam/zeebx-emu) and produces one file,
`zeebx.chimeraCore`, that goes in Chimera's `cores` folder.

## Status

Early. Two games have been tried, Quake II and Um Jogo de Ovos. Both run
from power-on to their menus with picture, sound, controllers and
savestates, on Linux and on Windows, and Quake II has been played into its
first level.

- The picture is drawn in software inside the core, so it is the same on
  every computer and needs no graphics card.
- The same input gives the same run every time.
- Quake II's level runs at about real time on a fast computer: the 3D
  picture is drawn in software on one processor thread.
- What a game saves can be exported from Chimera, and put back into a new
  project as its "Save data" file.

## What you bring

A game, as it was dumped from a console: a `.7z` or `.zip` archive with a
`mif` folder and a `mod` folder in it, or the game's `.mod` file. The
console's own system software is not needed. Zeebx's
[compatibility list](https://github.com/ZeebxTeam/zeebx-emu/blob/development/COMPATIBILIDADE.md)
says which games work.

## Controllers and settings

Two controllers, each with a D-pad, four buttons numbered as on the
controller, ZL, ZR, Start, Back and two sticks.

- **Controllers**: how many are plugged in, 1 or 2.
- **Clock at Power-On**: the date and time the console's clock starts at. A
  game that picks its random numbers from the time plays differently with
  another date.

RAM Watch and RAM Search see the game's memory as one bus, "Memory", at the
addresses the game itself uses.

## Building and testing

[docs/BUILDING.md](docs/BUILDING.md) has every step. [docs/PLAN.md](docs/PLAN.md)
is the design log: what was decided, why, and what was measured.

## Licence

This repository's own code is MIT. Zeebx is GPL-2.0-or-later, and the
patches to it here are under the same terms. The built package is under the
GPL version 3, as Zeebx's own binaries are; it carries the licence texts of
everything in it. See [LICENSE](LICENSE).
