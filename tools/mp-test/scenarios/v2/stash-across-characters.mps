# The shared stash is the account's (plan N28a): what one character puts
# in, another character of the same account takes out. One account plays
# one character at a time, so they play one after the other.
name    stash-across-characters
about   One character puts an item in; another of the same account takes it; the first no longer has it.
rule    STASH  the shared stash is the account's: every character of it sees and takes the same items (plan N28a)
rule    ONCE   the item is in exactly one place (plan N28a)
bots    1
setup   split:2
needs   node-games

all connect
b0  register ana ana-pass-1               expect ok                          rule STASH
b0  char-create Anapal 3                  expect ok                          rule STASH
b0  char-create Anasorc 1                 expect ok                          rule STASH
b0  char-select Anapal                    expect ok                          rule STASH
b0  create-game anagame                   expect ok                          rule STASH
server dev Anapal item cap
b0  inventory cap                         expect has.cap=yes       within 5s rule STASH
b0  stash-put cap                         expect ok                within 15s rule STASH
b0  inventory cap                         expect has.cap=no        within 5s   rule ONCE
b0  leave-game
b0  games                                 expect count=0           within 15s  rule STASH
b0  char-select Anasorc                   expect ok                          rule STASH
b0  stash cap                             expect cap.count=1                   rule STASH
b0  create-game sorcgame                  expect ok                          rule STASH
b0  stash-take cap                        expect ok                within 15s  rule STASH
b0  inventory cap                         expect has.cap=yes       within 5s   rule STASH
b0  stash cap                             expect cap.count=0                   rule ONCE
b0  leave-game
b0  games                                 expect count=0           within 15s  rule ONCE
b0  char-select Anapal                    expect ok                          rule STASH
b0  create-game palgame                   expect ok                          rule STASH
b0  inventory cap                         expect has.cap=no        within 5s   rule ONCE
b0  stash cap                             expect cap.count=0                   rule ONCE
