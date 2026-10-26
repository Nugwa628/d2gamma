# The game-server choice (plan N12, lobby contract 11.4). Entering the
# lobby brings the list of linked game servers (NodeList); the create form's
# choice goes with CreateGame: a named server runs the game, Automatic
# takes the least loaded one, and a server that is lost leaves the list and
# can't be chosen any more.
name   create-on-chosen-node
about  Both game servers are listed; games created on n2 and on n1 by choice run there, Automatic takes the less loaded one; n2 is killed, leaves the list and is refused by name.
rule   LIST    every linked game server is listed to the lobby, with its address and load (plan N12)
rule   CHOICE  a game created on a chosen game server runs on it (plan N12)
rule   AUTO    Automatic puts the game on the least loaded open game server (plan N12)
rule   LOST    a lost game server leaves the list, and choosing it is refused (plan N12)
bots   3
setup  split:2
needs  node-games node-choice

node n1    expect linked=yes                                   within 15s rule LIST
node n2    expect linked=yes                                   within 15s rule LIST
all connect
b0  register pia pia-pass-1                                    expect ok
b1  register rex rex-pass-1                                    expect ok
b2  register sol sol-pass-1                                    expect ok
b0  char-create Piapal 3                                       expect ok
b1  char-create Rexsorc 1                                      expect ok
b2  char-create Solpal 3                                       expect ok
b0  char-select Piapal                                         expect ok
b1  char-select Rexsorc                                        expect ok
b2  char-select Solpal                                         expect ok
b0  nodes      expect count=2 has.n1=yes has.n2=yes n1.state=open n2.state=open within 5s rule LIST
b1  nodes      expect n1.load=0 n2.load=0 suggested=n1         within 5s rule LIST
b0  create-game near on-node n2                                expect ok node n2  rule CHOICE
b1  games      expect near.node=n2                             within 5s rule CHOICE
b1  join-game near                                             expect ok node n2  rule CHOICE
b1  leave-game
# n2 runs a game now: its load goes up, n1 becomes the suggestion.
b2  nodes      expect n2.load>=1 suggested=n1                  within 15s rule LIST
b2  create-game auto1                                          expect ok node n1  rule AUTO
b2  leave-game
b0  leave-game
# near is empty and closes: its players are free for a new game.
b1  games      expect has.near=no                              within 15s rule CHOICE
b1  create-game chosen on-node n1 enter=no                     expect ok node n1  rule CHOICE
b0  create-game nowhere on-node n9 enter=no                    expect refused=NodeUnavailable rule CHOICE
kill-node n2
b0  nodes      expect count=1 has.n1=yes has.n2=no             within 30s rule LOST
b0  create-game gone on-node n2 enter=no                       expect refused=NodeUnavailable rule LOST
