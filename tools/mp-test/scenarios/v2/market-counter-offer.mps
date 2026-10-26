# Written ahead of the market; the market scenarios are in scenarios/market.
name    market-counter-offer
about   A counter offer replaces the first.
rule    MARKET  a counter offer ends the one it answers (plan N31)
setup   split:2
pending counters run as tools/mp-test/scenarios/market/offer-counter-accept.mps (K1c)
