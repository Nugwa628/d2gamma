# Written ahead of the market; the market scenarios are in scenarios/market.
name    market-expire-to-deliveries
about   An unsold listing expires to the seller's Deliveries.
rule    MARKET  an expired listing returns to its seller (plan N29a)
setup   split:2
pending expiry runs as tools/mp-test/scenarios/market/expire-to-deliveries.mps (K1c)
