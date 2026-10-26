# The online shared stash (plan N28b) against a hostile game server: in
# mode invent-stash-item it hands the main server every item put in the
# stash under an item ID the item registry never granted. The main
# server refuses it (and logs it); the game server gives the real item
# back, so nothing is lost or made.
name    stash-hostile-node-invents-item
about   A hostile game server puts an invented item in the stash; it is refused.
rule    STASH  the stash takes only items that exist (plan N28b)
rule    ONCE   the refused item stays the character's, and only once (plan N28b)
bots    1
setup   split:2
needs   node-games
hostile invent-stash-item

all connect
b0  register inva inva-pass-1             expect ok                          rule STASH
b0  char-create Invapal 3                 expect ok                          rule STASH
b0  char-select Invapal                   expect ok                          rule STASH
b0  create-game invagame                  expect ok node n1                  rule STASH
server dev Invapal item cap
b0  inventory cap                         expect has.cap=yes       within 5s rule STASH
b0  stash-put cap                         expect refused=TransferInvalid within 15s rule STASH
b0  stash cap                             expect cap.count=0 items=0           rule STASH
b0  inventory cap                         expect cap.n=1           within 5s   rule ONCE
# Its save still holds it, once.
b0  leave-game
b0  games                                 expect count=0           within 15s  rule ONCE
b0  create-game invaback                  expect ok                          rule STASH
b0  inventory cap                         expect cap.n=1           within 5s   rule ONCE
b0  stash cap                             expect cap.count=0 items=0           rule ONCE
