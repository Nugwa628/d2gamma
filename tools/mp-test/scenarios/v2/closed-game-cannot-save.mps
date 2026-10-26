# Written ahead: it needs a hostile game server that saves after its game
# closed. The main server's refusal itself is unit-tested
# (save_after_close_is_refused, plan N2).
name    closed-game-cannot-save
about   A game server whose game was closed sends a save for a seat in it; the main server refuses it.
rule    LEASE  a save for a seat whose lease ended is refused; the character stays as last saved (plan N2)
setup   split:2
pending hostile late-save: a hostile test-build mode that saves after its game closed, not built yet
