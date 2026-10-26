# The online shared stash keeps runes and gems as counts (plan N28b). A
# plain rune or gem put in goes to its slot: the slot's count rises by
# one, by the inventory's send button too. Taken out, some or all of a
# slot come to the inventory as plain single items; a slot never gives
# more than it keeps.
name    stash-runes-as-counts
about   Runes and gems are kept as counts.
rule    STASH  stackables are counts that never go below zero (plan N28b)
rule    ONCE   a rune put in or taken out is in exactly one place: the character or its slot (plan N28b)
bots    1
setup   split:2
needs   node-games

all connect
b0  register runa runa-pass-1             expect ok                          rule STASH
b0  char-create Runapal 3                 expect ok                          rule STASH
b0  char-select Runapal                   expect ok                          rule STASH
b0  create-game runagame                  expect ok node n1                  rule STASH
server dev Runapal item r01
server dev Runapal item r01
server dev Runapal item r01
server dev Runapal item gcv
b0  inventory r01 gcv                     expect r01.n=3 gcv.n=1   within 5s rule STASH
b0  stash r01 gcv                         expect slot.r01=0 slot.gcv=0 items=0  rule STASH
# One rune put in: its slot counts it; no item page holds it.
b0  stash-put r01                         expect ok count=1        within 15s  rule STASH
b0  inventory r01                         expect r01.n=2           within 5s   rule ONCE
b0  stash r01                             expect slot.r01=1 items=0            rule STASH
# The send button: every rune and gem it holds, in one request.
b0  stash-send r01 gcv                    expect ok count=3 left=0 within 15s  rule STASH
b0  inventory r01 gcv                     expect r01.n=0 gcv.n=0   within 5s   rule ONCE
b0  stash r01 gcv                         expect slot.r01=3 slot.gcv=1 items=0 rule STASH
# Some, then all: plain single items in the inventory.
b0  stash-take-stack r01 2                expect ok count=2        within 15s  rule STASH
b0  inventory r01                         expect r01.n=2           within 5s   rule ONCE
b0  stash r01                             expect slot.r01=1                    rule ONCE
b0  stash-take-stack r01 all              expect ok count=1        within 15s  rule STASH
b0  inventory r01                         expect r01.n=3           within 5s   rule ONCE
b0  stash r01 gcv                         expect slot.r01=0 slot.gcv=1         rule ONCE
# An empty slot gives nothing: never below zero.
b0  stash-take-stack r01 1                expect refused=ItemNotFound within 15s  rule STASH
b0  stash r01                             expect slot.r01=0                    rule STASH
b0  inventory r01                         expect r01.n=3           within 5s   rule ONCE
# The counts are kept with the account: a later game sees them.
b0  leave-game
b0  games                                 expect count=0           within 15s  rule ONCE
b0  create-game runaback                  expect ok                          rule STASH
b0  inventory r01                         expect r01.n=3           within 5s   rule ONCE
b0  stash gcv                             expect slot.gcv=1                    rule STASH
