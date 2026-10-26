# A stash move is never lost or copied (plan N28a). The game server is
# frozen while an item of the stash is on its way to the character, then
# killed: the main server sees the link go, ends the seat's lease and puts
# the item back in the stash. The character's save never held it.
name    stash-node-dies-mid-take
about   A game server freezes and dies while an item of the shared stash is on its way to a character; the item comes back to the stash, and the character doesn't have it.
rule    ONCE   a take cut by a game server's crash leaves the item in exactly one place: the stash (plan N28a)
bots    2
setup   split:2
needs   node-games

all connect
b0  register uma uma-pass-1               expect ok
b0  char-create Umapal 3                  expect ok
b0  char-select Umapal                    expect ok
b0  create-game umagame                   expect ok node n1
server dev Umapal item cap
b0  inventory cap                         expect has.cap=yes       within 5s
b0  stash-put cap                         expect ok                within 15s
b0  inventory cap                         expect has.cap=no        within 5s
b0  stash cap                             expect cap.count=1
pause-node n1
b0  stash-take cap answer=no              expect ok
b0  stash cap                             expect cap.count=0       within 5s
wait 1s
kill-node n1
b0  stash cap                             expect cap.count=1       within 30s  rule ONCE
b1  login uma uma-pass-1                  expect ok
b1  char-select Umapal                    expect ok
# The main server saw the game server go: its game is gone.
b1  games                                 expect count=0           within 30s
b1  create-game umaagain                  expect ok node n2
b1  inventory cap                         expect has.cap=no        within 5s   rule ONCE
b1  stash cap                             expect cap.count=1                   rule ONCE
b1  stash-take cap                        expect ok                within 15s  rule ONCE
b1  inventory cap                         expect has.cap=yes       within 5s   rule ONCE
b1  stash cap                             expect cap.count=0                   rule ONCE
