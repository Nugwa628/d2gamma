# Written ahead of plan module N13; pending until it lands.
name    zombie-node-discarded
about   A game server comes back after its leases were revoked; its old games are discarded.
rule    LOSS  a game server back from the dead can't write anything from before (plan N13)
setup   split:2
pending N13: not built yet
