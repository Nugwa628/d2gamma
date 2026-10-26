# The first games on a game server (plan N4). The main server starts the
# game on the least loaded game server (n1 when both are empty) and offers
# each seat there; each client plays on a connection of its own to it.
# The saves go back to the main server; the game server keeps no character
# files. A test setup's game servers take dev commands (dev = test).
name   remote-game-play
about  Two bots create and join a game on a game server, see each other, move and kill a monster, then leave; their saves reach the main server and outlive its restart; the game server keeps no character files.
rule   REMOTE  a game on a game server plays as a game on the main server did: both players see each other, the walk and the same monster's death (plan N4)
rule   SAVED   a player's saves reach the main server and outlive its restart; the game server keeps no character files (plan N4)
bots   2
setup  split:2
needs  node-games

all connect
b0  register pia pia-pass-1               expect ok
b1  register rex rex-pass-1               expect ok
b0  char-create Piapal 3                  expect ok
b1  char-create Rexsorc 1                 expect ok
b0  char-list                             expect Piapal.played=no              rule SAVED
b0  char-select Piapal                    expect ok
b1  char-select Rexsorc                   expect ok
b0  create-game faraway                   expect ok node n1                 rule REMOTE
b1  join-game faraway                     expect ok node n1                 rule REMOTE
b0  see                                   expect players=2 b1=yes  within 5s   rule REMOTE
b0  move +3 +0
b1  see                                   expect b0.moved=yes      within 3s   rule REMOTE
server dev Piapal god
server dev Rexsorc god
server dev Piapal warp 2
server dev Rexsorc warp 2
b0  area                                  expect area=blood-moor   within 5s   rule REMOTE
b1  see                                   expect b0=yes            within 5s   rule REMOTE
server dev Piapal spawn fallen
all monster fallen                        expect seen=yes          within 3s   rule REMOTE
all attack fallen                         expect ok attacks>=1     within 20s  rule REMOTE
b1  monster fallen                        expect dead=yes          within 3s   rule REMOTE
all leave-game
b0  char-list                             expect Piapal.played=yes within 5s   rule SAVED
b1  char-list                             expect Rexsorc.played=yes within 5s  rule SAVED
node n1                                   expect chars=0                       rule SAVED
restart-realm
b0  reconnect                             expect ok same=no                    rule SAVED
b0  login pia pia-pass-1                  expect ok                            rule SAVED
b0  char-list                             expect Piapal.played=yes             rule SAVED
