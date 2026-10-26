# Written ahead of plan module N7; pending until it lands.
name    journal-arrives
about   A game on a game server sends its journal; the main server stores it and replays it to the same state hashes.
rule    JOURNAL  every game's journal arrives at the main server and replays to the same state (plan N7)
setup   split:2
pending N7: not built yet
