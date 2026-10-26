# The realm ladder (lobby contract 6): a season opened on the realm
# console; a ladder character ranks in it and a non-ladder character in
# season 0, their rows written as their games save.
name   ladder-season
about  A test season starts; a ladder and a non-ladder character play and save; each lands in its own ladder with the level and experience of its save.
rule   SEASON  season-start opens a season the list shows as current, a test season marked so; a ladder character needs an open season (NoSuchSeason) (contract 6)
rule   OWN     a ladder character ranks in its season's ladder, a non-ladder character in season 0's, neither in the other's (contract 6)
rule   SAVED   a stored save writes the character's row with the level and experience read from the save (contract 6)
bots   2
setup  split:1
needs  node-games
server ladder=on ladder_refresh_s=0 limits=test

all connect
b0  register lara lara-pass-1             expect ok
b1  register nils nils-pass-1             expect ok
b0  ladder                                expect ok seasons=1 current=0 total=0               rule SEASON
b0  char-create Laraladder 3 ladder       expect refused=NoSuchSeason                         rule SEASON
server season-start Test one test
b0  ladder                                expect ok seasons=2 current=1 season.1.test=yes total=0   rule SEASON
b0  char-create Laraladder 3 ladder       expect ok                                           rule OWN
b1  char-create Nilsplain 1               expect ok                                           rule OWN
b0  char-select Laraladder                expect ok
b1  char-select Nilsplain                 expect ok
b0  create-game laragame                  expect ok
b1  create-game nilsgame                  expect ok
server dev Laraladder level 12
server dev Nilsplain level 7
all leave-game
b0  char-list                             expect Laraladder.played=yes   within 10s  rule SAVED
b1  char-list                             expect Nilsplain.played=yes    within 10s  rule SAVED
b0  ladder                                expect ok total=1 first=Laraladder first.level=12 first.exp>=1 Laraladder.me=yes my_rank=1   within 10s  rule SAVED
b1  ladder season=0                       expect ok total=1 first=Nilsplain first.level=7 first.exp>=1 Nilsplain.mine=yes            within 10s  rule SAVED
b1  ladder-find Laraladder                expect ok rank=1 Laraladder.mine=no                 rule OWN
b1  ladder-find Laraladder season=0       expect refused=UnknownPlayer                        rule OWN
b0  ladder-find Nilsplain                 expect refused=UnknownPlayer                        rule OWN
b0  ladder-find Nilsplain season=0        expect ok rank=1                                    rule OWN
