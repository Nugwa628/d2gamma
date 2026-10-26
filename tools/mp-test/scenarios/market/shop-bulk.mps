# The shop's bulk edits (contract 3.7): one request edits many listings,
# each with its own result; cancelled goods wait in Deliveries.
name   market-shop-bulk
about  Three listings are hidden, shown, noted and cancelled in bulk; buyers see them only while shown; the goods come back as three cancelled deliveries.
rule   BULK    a bulk edit answers per listing; hidden listings are in the Hidden tab and no buyer finds them (contract 3.7)
rule   CANCEL  cancelled listings' goods wait in Deliveries with the reason cancelled, and are claimed back (contract 3.7, 3.9)
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
b0  market-post r30:1 price=500           expect ok
b0  market-post r30:1 price=600           expect ok
b0  market-post r30:1 price=700           expect ok
b0  market-shop                           expect rows=3 shown=3                       rule BULK
b0  market-edit hide                      expect ok rows=3 rows_ok=3                  rule BULK
b0  market-shop tab=hidden                expect rows=3 hidden=3 shown=0              rule BULK
b1  market-search group=stackable         expect total=0                              rule BULK
b0  market-edit show tab=hidden           expect ok rows_ok=3                         rule BULK
b0  market-edit note fair                 expect ok rows_ok=3                         rule BULK
b1  market-search group=stackable         expect total=3 first.note=fair              rule BULK
b0  market-edit cancel                    expect ok rows_ok=3                         rule CANCEL
b1  market-search group=stackable         expect total=0                              rule CANCEL
b0  market-deliveries                     expect count=3 reasons=cancelled,cancelled,cancelled rule CANCEL
b0  market-claim                          expect ok rows_ok=3                         rule CANCEL
b0  market-deliveries                     expect count=0                              rule CANCEL
b0  market-post r30:3 price=900           expect ok                                   rule CANCEL
