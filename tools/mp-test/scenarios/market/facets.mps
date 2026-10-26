# A search's facets (contract 3.2): the counts by group and item type of
# what the search matches, from the facts the main server read.
name   market-facets
about  An item and a stackable are listed; a search with facets counts one of each group, and the item under its type.
rule   FACETS  a search with facets counts its matches by group and by item type, as the main server read them from the records (contract 3.2, K1c)
rule   FILTER  a group or type filter narrows the rows to it (contract 3.2)
bots   2
server market=on

all connect
b0  register sela sela-pass-1             expect ok
b0  char-create Selapal 3                 expect ok
b0  char-select Selapal                   expect ok
b0  create-game selagame                  expect ok
server dev Selapal item cap
b0  inventory cap                         expect has.cap=yes       within 5s  rule FACETS
b0  stash-put cap                         expect ok                within 15s
b0  leave-game
b0  games                                 expect count=0           within 15s  rule FACETS
server econ-seed sela r30:1
b0  market-post cap price=100             expect ok
b0  market-post r30:1 price=900           expect ok
b1  register byra byra-pass-1             expect ok
b1  char-create Byrasorc 1                expect ok
b1  char-select Byrasorc                  expect ok
b1  market-search facets=yes              expect total=2 groups=2 group.stackable=1 type.helm=1   rule FACETS
b1  market-search group=stackable         expect total=1 first.goods=r30:1                        rule FILTER
b1  market-search type=helm               expect total=1 first.group=other                        rule FILTER
