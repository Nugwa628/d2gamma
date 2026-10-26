# Written ahead of plan module N13; pending until it lands.
name    cut-link-node-closes-unsaved
about   A game server's link to the main server is cut for good; its games close and their later saves are refused.
rule    LOSS  a game server cut from the main server closes its games; nothing it sends after the loss is written (plan N13)
setup   split:2
pending N13: not built yet
