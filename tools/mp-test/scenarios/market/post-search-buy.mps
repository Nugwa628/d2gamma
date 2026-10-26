# A sale end to end (contract sections 3-5): an item found in a game goes
# to the shared stash, is listed, found by its name (facts the main
# server read from the record), bought, and both sides claim.
name   market-post-search-buy
about  A seller lists a stash item; a buyer finds it by name and buys it; the seller is told; item and gold move once each through Deliveries.
rule   POST    a posted stash item leaves the stash and is listed in the seller's economy (contract 3.6)
rule   SEARCH  a search finds a listing by the item's name, read by the main server from its record (contract 3.2, K1c)
rule   BUY     a buy at the listed price goes through once; the goods and the price wait in Deliveries and are claimed into the stashes (contract 3.4, 3.9)
rule   NOTICE  the seller, online, is told of the sale at once (contract 3.10)
rule   ONCE    item and gold are each in exactly one place: nothing is made or lost (contract 5)
bots   2
server market=on

all connect
b0  register sela sela-pass-1             expect ok
b0  char-create Selapal 3                 expect ok
b0  char-select Selapal                   expect ok
b0  create-game selagame                  expect ok
server dev Selapal item cap
b0  inventory cap                         expect has.cap=yes       within 5s  rule POST
b0  stash-put cap                         expect ok                within 15s
b0  leave-game
b0  games                                 expect count=0           within 15s  rule POST
b1  register byra byra-pass-1             expect ok
b1  char-create Byrasorc 1                expect ok
b1  char-select Byrasorc                  expect ok
server econ-seed byra gold:5000
b0  market                                expect on=yes why=on listings=0           rule POST
b0  market-post cap price=1200            expect ok                                 rule POST
b0  market                                expect listings=1                         rule POST
b0  stash cap                             expect cap.count=0                        rule ONCE
b1  market-search cap                     expect total=1 first.seller=Selapal first.price=1200 first.mine=no  rule SEARCH
b0  market-search cap                     expect total=1 first.mine=yes             rule SEARCH
b1  market-pick cap                       expect ok                                 rule BUY
b1  market-buy                            expect ok                                 rule BUY
b0  market                                expect notice=sold deliveries=1 listings=0 within 5s  rule NOTICE
b1  market-search cap                     expect total=0                            rule BUY
b1  market-deliveries                     expect count=1                            rule BUY
b1  market-claim                          expect ok rows_ok=1                       rule BUY
b1  stash cap                             expect cap.count=1 gold=3800              rule ONCE
b0  market-deliveries                     expect count=1 first.goods=gold:1200      rule BUY
b0  market-claim                          expect ok rows_ok=1                       rule BUY
b0  stash                                 expect gold=1200                          rule ONCE
b1  market-buy                            expect refused=ListingGone                rule ONCE
b1  market-deliveries                     expect count=0                            rule ONCE
