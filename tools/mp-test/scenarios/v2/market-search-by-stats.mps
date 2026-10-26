# Written ahead of the market; the market scenarios are in scenarios/market.
name    market-search-by-stats
about   A search by stats finds the matching listings.
rule    MARKET  search finds listings by their stats (plan N30)
setup   split:2
pending stat filters need a stat-search verb; the economy tests cover them (K1b); facets and filters run as tools/mp-test/scenarios/market/facets.mps (K1c)
