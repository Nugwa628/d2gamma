# stash-icon

Makes `crates/d2-client/art/send-stash.png`: the cel of the inventory's
"Send stackables to stash" button. The art is D2Gamma's own: an iron-banded
wooden chest with a brass lock under a gold bevelled down arrow. It is painted
by code (`paint.py`), with nothing copied or traced from game art.

```
py -3 -I tools/stash-icon/send_stash_icon.py <act 1 palette> [out.png]
```

`<act 1 palette>` is `data/global/palette/act1/pal.dat` from your own game
files (768 bytes), or a 16 x 16 swatch image of it. Only the palette indices
are written, never the palette itself. The output is the same on every run
(fixed seeds). It needs `numpy` and `Pillow`.

The PNG is 96 x 32, 8-bit greyscale, holding three 32 x 32 frames side by side:

| Frame | State | Look |
|---|---|---|
| 0 | normal | the art |
| 1 | hover | one palette step brighter |
| 2 | pressed | darker |

Each value is an act 1 palette index, and 0 is transparent. The client reads
the PNG into a cel at start-up (`crates/d2-client/src/ui/own_art.rs`). It draws
the cel over the game's 32 px button frame (`panel\buysellbtn` frame 0, or
frame 1 when pressed), which is read from the player's archives at run time.
The pressed cel is drawn 1 px down-right.
