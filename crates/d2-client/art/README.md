# D2Gamma's own UI art

Small cels the client draws beside the game's panel art. No game art is kept
here: game art is read from the player's own files at run time.

| File | What | Made by |
|---|---|---|
| `send-stash.png` | The inventory's "Send stackables to stash" icon: normal, hover and pressed frames | `tools/stash-icon` |

Each value in these images is an act 1 palette index (0 transparent); see
`src/ui/own_art.rs`.
