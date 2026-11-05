# The online shared stash on a realm of the game's own records
# (`records = game`, a mod realm): the realm can't read a character's
# record, so a move's record is written to the character's file with the
# summary its game server reported. A character saved with an item puts it
# in the stash, and its game server dies before any other save: the
# character's file is the move's record (without the item), so the item is
# in exactly one place.
name    stash-game-records-node-dies-after-put
about   On a realm of the game's own records a character saved with an item puts it in the shared stash and its game server dies before any other save; the item is in the stash and not on the character.
rule    ONCE   an item put in the stash is in exactly one place, on a realm of the game's own records too, when the game server dies after the put (plan N28a)
bots    2
setup   split:2
needs   node-games
server  records=game

all connect
b0  register vala vala-pass-1             expect ok
b0  char-create Valapal 3                 expect ok
b0  char-select Valapal                   expect ok
b0  create-game valagame on-node n1       expect ok node n1
server dev Valapal item cap
b0  inventory cap                         expect has.cap=yes       within 5s
b0  leave-game
b0  games                                 expect count=0           within 15s
b0  create-game valaput on-node n1        expect ok node n1
b0  inventory cap                         expect has.cap=yes       within 5s
b0  stash-put cap                         expect ok count=1        within 15s
b0  inventory cap                         expect has.cap=no        within 5s
b0  stash cap                             expect cap.count=1
kill-node n1
b1  login vala vala-pass-1                expect ok
b1  char-select Valapal                   expect ok
b1  games                                 expect count=0           within 30s
b1  create-game valaagain on-node n2      expect ok node n2
b1  inventory cap                         expect has.cap=no        within 5s   rule ONCE
b1  stash cap                             expect cap.count=1                   rule ONCE
