D2Gamma playtest build (Windows)
==============================

Build of commit @COMMIT@ (branch @REF@).
A private development build of D2Gamma, a Diablo II: Lord of
Destruction reimplementation. It contains no game files: it reads your
own Diablo II: Lord of Destruction install.

Start
-----
1. Copy D2Gamma-Game.exe into your Diablo II folder,
   the one holding Game.exe and d2data.mpq.
2. Double-click D2Gamma-Game.exe. The main menu opens.

Or keep it anywhere and point it at the game:
   D2Gamma-Game.exe --game-dir "C:\Program Files (x86)\Diablo II"
(or set the D2_GAME_DIR environment variable to that folder).

Shortcuts straight into a game (from a command prompt):
   D2Gamma-Game.exe --new sorceress MyName     new character
   D2Gamma-Game.exe --save "path\to\MyName.d2s" load a save
   D2Gamma-Game.exe --difficulty nightmare ...  normal / nightmare / hell
   D2Gamma-Game.exe --res 640x480               the 640 x 480 frame

Characters are saved in Documents\D2Gamma\saves (never inside the game
folder). An older Documents\D2Gamma (or d2rs) folder is moved to
Documents\D2Gamma on the first start, with its characters, shared stash,
settings and baked animations; if it cannot be moved it is copied and
the old folder is left as it was.
Key bindings: Documents\D2Gamma\controls.toml (Diablo II's default
keys).

Dev tools (not part of Diablo II; on by default in this alpha)
-------------------------------------------------------------
Press ` (backquote, left of 1) to open or close the dev panel (Esc also
closes it): click its buttons (god, heal, speed, level, gold, points,
waypoints, acts, towns, warp list per act, monster/item picker). While
it is open the game gets no keys or clicks. Or type a command in the
console at the bottom left: Enter runs it; Up/Down history, Tab completes. Turn the tools off with
--no-dev or the environment variable D2RUST_DEV=0.

   help [command]            list commands / usage
   god [on|off]              no death, life/mana refilled every tick
   heal                      full life, mana, stamina
   level <1..99> | +<n>      raise to that level / n levels
   gold <n>                  add gold (capped at level x 10000)
   statpts <n>, skillpts <n> add stat / skill points
   town <1..5>               warp to an act's town
   warp <level id|name>      warp to a level (warp 35, warp cold plains)
   waypoints                 know every waypoint
   acts                      open acts 2-5 (then travel by waypoint)
   item <code|name> [quality] [ilvl <n>] [ground]
                             create an item (item hax magic ilvl 30)
   identify                  identify everything you carry
   kill [screen|level]       kill monsters near you / in the level
   spawn <monster> [count]   spawn monsters (spawn fallen 5)
   reveal                    (not built yet)
   speed <percent>           faster walk/run (speed 0: off)
   nocd [on|off]             no skill cooldown
   freemana [on|off]         mana refilled every tick
   difficulty <name>         how to restart on another difficulty
   info                      info overlay (area, position, tick, FPS)
   bugreport [note]          bug report folder (screenshot, log, state)
   save <name>, load [name]  save / load a slot (load restarts there)
   clear                     clear the console

When it crashes
---------------
A message box names the reason. The details are in
Documents\D2Gamma\logs: crash.log (backtrace and what it was doing) and
game.log (everything it printed). Send both, a screenshot, and the save
(Documents\D2Gamma\saves\<name>.d2s). Full guide: docs/PLAYTEST.md in the
repository.

Font
----
The HD Text font built into D2Gamma-Game.exe is Marcellus SC, under the SIL
Open Font License 1.1:

Copyright (c) 2012, Brian J. Bonislawsky DBA Astigmatic (AOETI) (astigma@astigmatic.com), with Reserved Font Names "Marcellus"

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded, 
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
