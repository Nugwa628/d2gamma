# Price history (contract 3.11, K5): sales of a stackable make its
# series; what is listed now counts apart.
name   market-price-history
about  Two runes sell at 500 and 700 and a third is listed at 900; the rune's history shows two sales in one series, two recent sales and one listed now.
rule   HISTORY a stackable's history counts its sales in the span, its recent sales and what is listed now, in one series per price shape (contract 3.11)
bots   2
server market=on

all connect
b0  register sela sela-pass-1             expect ok
b0  char-create Selapal 3                 expect ok
b0  char-select Selapal                   expect ok
b1  register byra byra-pass-1             expect ok
b1  char-create Byrasorc 1                expect ok
b1  char-select Byrasorc                  expect ok
server econ-seed sela r30:3
server econ-seed byra gold:5000
b0  market-post r30:1 price=500           expect ok
b1  market-pick group=stackable           expect ok
b1  market-buy                            expect ok
b0  market-post r30:1 price=700           expect ok
b1  market-pick group=stackable           expect ok
b1  market-buy                            expect ok
b0  market-post r30:1 price=900           expect ok
b1  market-prices r30                     expect sold=2 listed_now=1 series=1 recent=2 s0.sold=2  rule HISTORY
b1  market-pick group=stackable           expect ok price=900                     rule HISTORY
b1  market-prices picked                  expect sold=2 listed_now=1               rule HISTORY
