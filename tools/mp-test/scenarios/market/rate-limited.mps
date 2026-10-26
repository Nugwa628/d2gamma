# Rate limits (contract 3.1, section 9): searches per minute per account;
# the one over the limit is refused with how long to wait.
name   market-rate-limited
about  An account sends 61 searches in a row at 60 a minute: 60 are answered and the 61st is RateLimited with a wait.
rule   LIMIT   the search over market_searches_per_min within a minute is refused RateLimited with wait_s above 0 (contract 3.1, 9)
bots   1
server market=on market_searches_per_min=60

b0  connect
b0  register sela sela-pass-1             expect ok
b0  char-create Selapal 3                 expect ok
b0  char-select Selapal                   expect ok
b0  market                                expect searches_per_min=60
b0  market-flood 61                       expect ok ok=60 limited=1 wait_s>=1    within 30s
b0  market-search                         expect refused=RateLimited wait_s>=1
b0  market-deliveries                     expect ok count=0
