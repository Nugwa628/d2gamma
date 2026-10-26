# Offers (contract 3.5): a buyer offers below the price, the seller
# counters, the buyer accepts the counter; each step is pushed to the
# other side.
name   market-offer-counter-accept
about  A buyer offers 600 on a 1000 listing; the seller counters 800; the buyer accepts; the seller gets 800 and the buyer the goods.
rule   OFFER    an offer holds its price out of the buyer's stash and reaches the seller at once (contract 3.5, 3.10)
rule   COUNTER  a counter reaches the buyer, who may accept it: the sale goes through at the counter's price (contract 3.5)
rule   ONCE     every gold piece is in one place: the seller gets the counter's price, the buyer pays it once (contract 5)
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
b0  market-post r30:1 price=1000 mode=both expect ok
b1  market-pick group=stackable           expect ok                                   rule OFFER
b1  market-offer price=600                expect ok                                   rule OFFER
b1  market                                expect my_offers=1                          rule OFFER
b1  market-get                            expect my_offer=open                        rule OFFER
b1  stash                                 expect gold=4400                            rule OFFER
b0  market                                expect notice=offer offers_to_me=1 within 5s rule OFFER
b0  market-offers                         expect count=1 first.offered=600 first.buyer=Byrasorc rule OFFER
b0  market-answer counter price=800       expect ok                                   rule COUNTER
b1  market                                expect notice=countered within 5s           rule COUNTER
b1  market-offers made=yes                expect count=1 first.state=countered first.counter=800 rule COUNTER
b1  market-answer accept side=made        expect ok                                   rule COUNTER
b0  market                                expect notice=sold deliveries=1 within 5s   rule COUNTER
b0  market-claim                          expect ok                                   rule ONCE
b0  stash                                 expect gold=800                             rule ONCE
b1  market-claim                          expect ok                                   rule ONCE
b1  stash                                 expect gold=4200                            rule ONCE
b1  market-deliveries                     expect count=0                              rule ONCE
