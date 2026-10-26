# A stash move is never lost or copied (plan N28a). The game server is
# frozen when the main server asks it for an item to put in the stash,
# then killed: nothing reached the stash, and the character's last save
# still holds the item.
name    stash-node-dies-mid-put
about   A game server freezes and dies during a put; the item stays the character's and is not in the stash.
rule    ONCE   a put cut by a game server's crash leaves the item in exactly one place: the character (plan N28a)
bots    2
setup   split:2
needs   node-games

all connect
b0  register ida ida-pass-1               expect ok
b0  char-create Idapal 3                  expect ok
b0  char-select Idapal                    expect ok
b0  create-game idagame                   expect ok node n1
server dev Idapal item cap
b0  inventory cap                         expect has.cap=yes       within 5s
# A save that holds the item.
b0  leave-game
b0  games                                 expect count=0           within 15s
b0  create-game idaagain                  expect ok node n1
b0  inventory cap                         expect has.cap=yes       within 5s
pause-node n1
b0  stash-put cap answer=no               expect ok
wait 1s
kill-node n1
b1  login ida ida-pass-1                  expect ok
b1  char-select Idapal                    expect ok
# The main server saw the game server go: its game is gone.
b1  games                                 expect count=0           within 30s
b1  stash cap                             expect cap.count=0                   rule ONCE
b1  create-game idalast                   expect ok node n2
b1  inventory cap                         expect has.cap=yes       within 5s   rule ONCE
b1  stash cap                             expect cap.count=0                   rule ONCE
