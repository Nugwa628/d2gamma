# Two buyers on one listing (contract 3.4): the first buy takes it; the
# second, which saw the same row, is refused and pays nothing.
name   market-buy-race
about  Two buyers pick the same listing; the first buys it, the second gets ListingGone and keeps its gold.
rule   ONE     a listing is sold once: the second buyer is refused ListingGone (contract 3.4)
rule   ONCE    the refused buyer pays nothing; the winner pays once (contract 5)
bots   3
server market=on

all connect
b0  register sela sela-pass-1             expect ok
b0  char-create Selapal 3                 expect ok
b0  char-select Selapal                   expect ok
b1  register byra byra-pass-1             expect ok
b1  char-create Byrasorc 1                expect ok
b1  char-select Byrasorc                  expect ok
b2  register cora cora-pass-1             expect ok
b2  char-create Corabar 4                 expect ok
b2  char-select Corabar                   expect ok
server econ-seed sela r30:1
server econ-seed byra gold:5000
server econ-seed cora gold:5000
b0  market-post r30:1 price=900           expect ok
b1  market-pick group=stackable           expect ok seller=Selapal              rule ONE
b2  market-pick group=stackable           expect ok seller=Selapal              rule ONE
b1  market-buy                            expect ok                             rule ONE
b2  market-buy                            expect refused=ListingGone            rule ONE
b2  market-deliveries                     expect count=0                        rule ONCE
b2  stash                                 expect gold=5000                      rule ONCE
b1  stash                                 expect gold=4100                      rule ONCE
b1  market-deliveries                     expect count=1 first.goods=r30:1      rule ONCE
