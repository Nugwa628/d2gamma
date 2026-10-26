# Written ahead of the market; the market scenarios are in scenarios/market.
name    market-buy-race
about   Two players buy the same listing at once; one gets it.
rule    MARKET  a listing sells once (plan N29a)
setup   split:2
pending the race runs as tools/mp-test/scenarios/market/buy-race.mps (K1c); a split:2 run waits for the market on more game servers
