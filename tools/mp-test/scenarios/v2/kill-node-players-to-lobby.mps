# Written ahead of plan module N13; pending until it lands.
name    kill-node-players-to-lobby
about   A game server is killed mid-game; its players are back in the lobby, saved as of the last save.
rule    LOSS  players of a lost game server land in the lobby with their last saved characters (plan N13)
setup   split:2
pending N13: not built yet
