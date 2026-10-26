# The online shared stash (plan N28b): an item taken from the stash to
# the cursor, as picking it up from the panel does, arrives on the
# character's cursor, not in its inventory.
name    stash-take-to-cursor
about   An item taken from the shared stash to the cursor lands on the cursor.
rule    STASH  a take asked for the cursor lands on the cursor (plan N28b)
rule    ONCE   the item is in exactly one place: the character or the stash (plan N28b)
bots    1
setup   split:2
needs   node-games

all connect
b0  register cura cura-pass-1             expect ok                          rule STASH
b0  char-create Curapal 3                 expect ok                          rule STASH
b0  char-select Curapal                   expect ok                          rule STASH
b0  create-game curagame                  expect ok node n1                  rule STASH
server dev Curapal item cap
b0  inventory cap                         expect has.cap=yes cap.cursor=no within 5s rule STASH
b0  stash-put cap                         expect ok count=1        within 15s  rule STASH
b0  inventory cap                         expect has.cap=no        within 5s   rule ONCE
b0  stash-take cap to=cursor              expect ok count=1        within 15s  rule STASH
b0  inventory cap                         expect cap.n=1 cap.cursor=yes within 5s rule STASH
b0  stash cap                             expect cap.count=0 items=0           rule ONCE
