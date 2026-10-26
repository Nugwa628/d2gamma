# Written ahead of the market; the market scenarios are in scenarios/market.
name    market-offer-items
about   An offer of items for items moves both ways.
rule    MARKET  an accepted offer moves both sides once (plan N31)
setup   split:2
pending item offers need item verbs in offers; the economy tests cover them (K1b, K4); the offer flow runs as tools/mp-test/scenarios/market/offer-counter-accept.mps (K1c)
