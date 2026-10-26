# A buy names the price it saw (contract 3.4, the price hash): when the
# seller changes it in between, the buy is refused and nothing moves.
name   market-price-changed
about  The seller reprices after the buyer looked; the buy at the old price is PriceChanged; a fresh look buys at the new one.
rule   HASH    a buy at a price the seller changed since is refused PriceChanged and moves nothing (contract 3.4)
rule   REPRICE a bulk reprice changes the listing's price for every buyer (contract 3.7)
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
b0  market-post r30:1 price=500           expect ok
b1  market-pick group=stackable           expect ok price=500                   rule HASH
b0  market-edit reprice 700               expect ok rows_ok=1                   rule REPRICE
b1  market-buy                            expect refused=PriceChanged           rule HASH
b1  stash                                 expect gold=5000                      rule HASH
b1  market-search group=stackable         expect total=1 first.price=700        rule REPRICE
b1  market-pick group=stackable           expect ok price=700                   rule REPRICE
b1  market-buy                            expect ok                             rule REPRICE
b1  stash                                 expect gold=4300                      rule REPRICE
