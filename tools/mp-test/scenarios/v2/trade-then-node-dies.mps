# Written ahead of plan module N25; pending until it lands.
name    trade-then-node-dies
about   Two players trade, then their game server dies; the item exists exactly once.
rule    BATCH  a trade is written whole or not at all (plan N25)
setup   split:2
pending N25: not built yet
