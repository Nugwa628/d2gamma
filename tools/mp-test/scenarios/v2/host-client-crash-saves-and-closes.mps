# Written ahead of plan module N8; pending until it lands.
name    host-client-crash-saves-and-closes
about   The host's game client crashes; its hosting process saves every seat, closes the game and exits.
rule    HOST  a hosting process whose client is gone saves, closes the game and exits by itself (plan N8)
setup   host
pending N8: not built yet
