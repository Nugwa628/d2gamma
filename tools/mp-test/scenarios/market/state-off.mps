# The market switch is off by default (contract section 9): the hello
# still answers, saying why, and every other market request is refused.
name   market-state-off
about  With the switch off (the default) the hello's state is off (switch) and every other request is MarketOff; with no character selected it is off (no-character) and NoCharSelected.
rule   OFF     the market is off by default: the hello says on=no for the switch and every other market request is refused MarketOff (contract 3, 9)
rule   NOCHAR  with no character selected the hello says on=no for it and every other market request is refused NoCharSelected (contract 3)
bots   1

b0  connect
b0  register offa offa-pass-1             expect ok
b0  market                                expect on=no why=no-character    rule NOCHAR
b0  market-search                         expect refused=NoCharSelected    rule NOCHAR
b0  char-create Offapal 3                 expect ok
b0  char-select Offapal                   expect ok
b0  market                                expect on=no why=switch          rule OFF
b0  market-search                         expect refused=MarketOff         rule OFF
b0  market-deliveries                     expect refused=MarketOff         rule OFF
b0  market-claim                          expect refused=MarketOff         rule OFF
b0  market-post r01:1 price=10            expect refused=MarketOff         rule OFF
