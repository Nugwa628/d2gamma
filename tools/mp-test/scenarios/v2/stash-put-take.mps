# The online shared stash (plan N28a). A character in a game on a game
# server puts an item in its account's stash: the game server takes it off
# the character and hands it to the main server with the save that no
# longer holds it. The stash outlives a main-server restart; taken back,
# the item arrives in the inventory with a save that holds it, and that
# save is the character's from then on.
name    stash-put-take
about   A character on a game server puts an item in the shared stash; it is there after a main-server restart; taken back, it is in the inventory and stays there in the character's save.
rule    STASH  the shared stash keeps what is put in, across a main-server restart (plan N28a)
rule    ONCE   an item put in or taken out is in exactly one place: the character or the stash (plan N28a)
bots    1
setup   split:2
needs   node-games

all connect
b0  register sia sia-pass-1               expect ok                          rule STASH
b0  char-create Siapal 3                  expect ok                          rule STASH
b0  char-select Siapal                    expect ok                          rule STASH
b0  create-game siagame                   expect ok node n1                  rule STASH
server dev Siapal item cap
b0  inventory cap                         expect has.cap=yes       within 5s rule STASH
b0  stash cap                             expect has.cap=no items=0          rule STASH
b0  stash-put cap                         expect ok count=1        within 15s  rule STASH
b0  inventory cap                         expect has.cap=no        within 5s   rule ONCE
b0  stash cap                             expect has.cap=yes cap.count=1 items=1  rule STASH
b0  leave-game
restart-realm
node n1                                   expect linked=yes links=2 within 30s  rule STASH
node n2                                   expect linked=yes links=2 within 30s  rule STASH
b0  reconnect                             expect ok same=no                  rule STASH
b0  login sia sia-pass-1                  expect ok                          rule STASH
b0  char-select Siapal                    expect ok                          rule STASH
b0  stash cap                             expect cap.count=1                   rule STASH
b0  create-game siaback                   expect ok                          rule STASH
b0  inventory cap                         expect has.cap=no        within 5s   rule ONCE
b0  stash-take cap                        expect ok count=1        within 15s  rule STASH
b0  inventory cap                         expect has.cap=yes       within 5s   rule ONCE
b0  stash cap                             expect cap.count=0 items=0           rule ONCE
b0  leave-game
b0  games                                 expect count=0           within 15s  rule ONCE
b0  create-game siaagain                  expect ok                          rule STASH
b0  inventory cap                         expect has.cap=yes       within 5s   rule ONCE
b0  stash cap                             expect cap.count=0                   rule ONCE
