# The game list names each game's game server (plan N12): the id the
# client finds in the game-server list, and the region players see, the
# same name the list gives that server.
name    join-sees-node
about   Games created on n1 and on n2 are listed with the game server each runs on.
rule    CHOICE  the lobby shows each game's game server (plan N12)
bots    2
setup   split:2
needs   node-games node-choice

node n1    expect linked=yes                                   within 15s rule CHOICE
node n2    expect linked=yes                                   within 15s rule CHOICE
all connect
b0  register pia pia-pass-1                                    expect ok
b1  register rex rex-pass-1                                    expect ok
b0  char-create Piapal 3                                       expect ok
b1  char-create Rexsorc 1                                      expect ok
b0  char-select Piapal                                         expect ok
b1  char-select Rexsorc                                        expect ok
b0  create-game west on-node n1                               expect ok node n1  rule CHOICE
b1  create-game east on-node n2                                expect ok node n2  rule CHOICE
b0  games      expect west.node=n1 east.node=n2 west.hosted=no east.hosted=no within 5s rule CHOICE
b1  nodes      expect has.n1=yes has.n2=yes                    within 5s rule CHOICE
