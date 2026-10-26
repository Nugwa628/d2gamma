# Written ahead of the market; the market scenarios are in scenarios/market.
name    market-post-buy
about   A player lists an item; another buys it.
rule    MARKET  a sale moves item and price once each (plan N29a)
setup   split:2
pending a sale runs as tools/mp-test/scenarios/market/post-search-buy.mps (K1c)
