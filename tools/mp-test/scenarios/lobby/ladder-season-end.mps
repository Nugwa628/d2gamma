# Season end (lobby contract 6, plan lobby-ui L6b): an ended season keeps
# its final ranks, browsable; its characters become non-ladder when next
# loaded for a game, ranked in season 0 from then on.
name   ladder-season-end
about  A season ends with one character in the lobby and one in a game; the ended season keeps its final ranks; each character moves to season 0 when it next enters a game; the next season's characters stay in theirs.
rule   FINAL   an ended season stays listed, marked ended, with its final ranks; a save after its end doesn't change them (contract 6)
rule   RETIRE  a character of an ended season becomes non-ladder when next loaded: season 0, ranked in season 0's ladder from its record, its later saves there (contract 6, L6b)
rule   NEXT    the next season's characters stay in it; a character that left an ended season stays non-ladder (contract 6)
bots   2
setup  split:1
needs  node-games
server ladder=on ladder_refresh_s=0 limits=test

all connect
b0  register lara lara-pass-1             expect ok
b1  register nils nils-pass-1             expect ok
server season-start Season one
b0  char-create Laraladder 3 ladder       expect ok
b1  char-create Nilsladder 1 ladder       expect ok
b0  char-select Laraladder                expect ok
b1  char-select Nilsladder                expect ok
b0  create-game laragame                  expect ok
b1  create-game nilsgame                  expect ok
server dev Laraladder level 12
server dev Nilsladder level 7
all leave-game
b0  ladder                                expect ok current=1 total=2 first=Laraladder Laraladder.level=12 Nilsladder.level=7   within 10s  rule FINAL
# Nils is in a game when the season ends.
b1  create-game nilsgame2                 expect ok
server season-end
b0  ladder season=1                       expect ok current=0 season.1.ended=yes total=2 Laraladder.rank=1 Laraladder.level=12 Nilsladder.rank=2   rule FINAL
b0  char-list                             expect Laraladder.season=1                                  rule RETIRE
server dev Nilsladder level 9
b1  leave-game
b1  char-list                             expect Nilsladder.level=9 Nilsladder.season=1   within 10s  rule FINAL
b1  ladder-find Nilsladder season=1       expect ok rank=2 Nilsladder.level=7                         rule FINAL
b1  ladder-find Nilsladder season=0       expect refused=UnknownPlayer                                rule RETIRE
# Loaded for a game: non-ladder from now on.
b0  create-game laragame2                 expect ok                                                   rule RETIRE
b0  char-list                             expect Laraladder.season=0                                  rule RETIRE
b0  ladder season=0                       expect ok total=1 first=Laraladder first.level=12 first.exp>=1   rule RETIRE
server dev Laraladder level 15
b0  leave-game
b0  ladder season=0                       expect ok total=1 first=Laraladder first.level=15   within 10s  rule RETIRE
b1  create-game nilsgame3                 expect ok                                                   rule RETIRE
b1  char-list                             expect Nilsladder.season=0                                  rule RETIRE
b1  ladder-find Nilsladder season=0       expect ok rank=2 Nilsladder.level=9                         rule RETIRE
b1  leave-game
b0  ladder season=1                       expect ok total=2 Laraladder.level=12 Nilsladder.level=7    rule FINAL
# The next season.
server season-start Season two
b0  char-create Laranext 3 ladder         expect ok                                                   rule NEXT
b0  char-select Laranext                  expect ok
b0  create-game laragame3                 expect ok                                                   rule NEXT
server dev Laranext level 4
b0  leave-game
b0  ladder                                expect ok current=2 total=1 first=Laranext first.level=4   within 10s  rule NEXT
b0  char-list                             expect Laranext.season=2 Laraladder.season=0                rule NEXT
b0  ladder season=1                       expect ok season.1.ended=yes total=2 Laraladder.level=12    rule FINAL
