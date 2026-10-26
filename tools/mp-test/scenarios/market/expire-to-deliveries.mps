# Expiry (contract 3.8): a listing past its days goes back to the seller's
# Deliveries, an offer on it back to its buyer's; both are told. The test
# server's market clock moves forward (`econ-clock`) instead of waiting.
name   market-expire-to-deliveries
about  A one-day listing with an open offer expires when the clock moves two days on: the seller and the buyer get their goods back as expired deliveries and a notice each.
rule   EXPIRE  a listing past its time leaves the market; its goods wait in the seller's Deliveries, an open offer's in its buyer's, with the reason expired (contract 3.8)
rule   NOTICE  both sides, online, are told (contract 3.10)
rule   ONCE    nothing is made or lost (contract 5)
bots   2
server market=on

all connect
b0  register sela sela-pass-1             expect ok
b0  char-create Selapal 3                 expect ok
b0  char-select Selapal                   expect ok
b1  register byra byra-pass-1             expect ok
b1  char-create Byrasorc 1                expect ok
b1  char-select Byrasorc                  expect ok
server econ-seed sela r30:1
server econ-seed byra gold:5000
b0  market-post r30:1 price=1000 mode=both days=1 expect ok
b1  market-pick group=stackable           expect ok
b1  market-offer price=400                expect ok
server econ-clock +2d
b0  market                                expect notice=expired deliveries=1 listings=0 within 5s rule NOTICE
b1  market                                expect notice=returned deliveries=1 my_offers=0 within 5s rule NOTICE
b1  market-search group=stackable         expect total=0                              rule EXPIRE
b0  market-shop tab=expired               expect rows=1 first.state=expired           rule EXPIRE
b0  market-deliveries                     expect count=1 first.reason=expired first.goods=r30:1 rule EXPIRE
b1  market-deliveries                     expect count=1 first.reason=expired first.goods=gold:400 rule EXPIRE
b1  market-claim                          expect ok rows_ok=1                         rule ONCE
b1  stash                                 expect gold=5000                            rule ONCE
b0  market-claim                          expect ok rows_ok=1                         rule ONCE
b0  market-deliveries                     expect count=0                              rule ONCE
