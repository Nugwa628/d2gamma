# The split setup comes up: the main server and two game-server processes,
# each with its own config and identity, linked (plan N1).
name   node-links
about  The main server starts, then n1 and n2; each prints its ready line and links to the main server.
rule   LINK  every game server of a split setup links to the main server after it starts (contracts 11.10)
setup  split:2

node n1    expect running=yes linked=yes starts=1 node n1   within 15s
node n2    expect running=yes linked=yes starts=1 node n2   within 15s
