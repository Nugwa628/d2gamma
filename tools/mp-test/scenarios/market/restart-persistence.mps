# The market survives a restart (contract 5): listings, offers and request
# numbers are in realm.sqlite, not in memory.
name   market-restart-persistence
about  A listing with an offer survives a clean restart: after logging in again the buyer still sees it and its offer, and buys it; request numbers go on from where they were.
rule   KEEP    listings, offers and the account's last request number survive a main server restart (contract 3.1, 5)
bots   2
server market=on
needs  persist

all connect
b0  register sela sela-pass-1             expect ok
b0  char-create Selapal 3                 expect ok
b0  char-select Selapal                   expect ok
b1  register byra byra-pass-1             expect ok
b1  char-create Byrasorc 1                expect ok
b1  char-select Byrasorc                  expect ok
server econ-seed sela r30:1
server econ-seed byra gold:5000
b0  market-post r30:1 price=900 mode=both expect ok
b1  market-pick group=stackable           expect ok
b1  market-offer price=300                expect ok
b1  market                                expect last_req>=2
server restart
all reconnect                             expect ok
b0  login sela sela-pass-1                expect ok
b0  char-select Selapal                   expect ok
b1  login byra byra-pass-1                expect ok
b1  char-select Byrasorc                  expect ok
b0  market                                expect listings=1 offers_to_me=1        rule KEEP
b1  market                                expect my_offers=1 last_req>=2          rule KEEP
b1  market-get                            expect state=active my_offer=open       rule KEEP
b1  market-buy                            expect ok                               rule KEEP
b1  market-deliveries                     expect count=2 reasons=none,outbid      rule KEEP
b1  stash                                 expect gold=3800                        rule KEEP
