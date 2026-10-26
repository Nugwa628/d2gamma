# Written ahead of plan module N21; pending until it lands.
name    update-node-rolling
about   Game servers are updated one at a time; play goes on.
rule    OPS  a rolling update loses no game (plan N21)
setup   split:2
pending N21: not built yet
