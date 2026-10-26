# Request numbers (contract 3.1): a change sent again with the same
# number gets its stored answer back and changes nothing twice.
name   market-repeat-req
about  A post and a buy are each sent twice with the same request number: the second answer says repeated, one listing exists and the buyer pays once.
rule   ONCE    a repeated change request is answered from its stored result with repeated=yes and is not done again (contract 3.1)
bots   2
server market=on

all connect
b0  register sela sela-pass-1             expect ok
b0  char-create Selapal 3                 expect ok
b0  char-select Selapal                   expect ok
b1  register byra byra-pass-1             expect ok
b1  char-create Byrasorc 1                expect ok
b1  char-select Byrasorc                  expect ok
server econ-seed sela r30:2
server econ-seed byra gold:5000
b0  market-post r30:1 price=500           expect ok repeated=no
b0  market-repeat                         expect ok repeated=yes
b0  market                                expect listings=1
b1  market-search group=stackable         expect total=1
b1  market-pick group=stackable           expect ok
b1  market-buy                            expect ok repeated=no
b1  market-repeat                         expect ok repeated=yes
b1  stash                                 expect gold=4500
b1  market-deliveries                     expect count=1
b0  market                                expect deliveries=1
