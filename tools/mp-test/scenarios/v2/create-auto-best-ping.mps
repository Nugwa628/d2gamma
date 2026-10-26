# Written ahead of plan module N12; pending until it lands.
name    create-auto-best-ping
about   With n1 at 20 ms and n2 at 80 ms, a game created on auto goes to n1.
rule    CHOICE  auto picks the game server with the best ping (plan N12)
setup   split:2
pending N12: not built yet
