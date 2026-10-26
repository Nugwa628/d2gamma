# The chat's rate limits (lobby contract 9). The new-recipient limit (10
# new accounts per 10 minutes) takes 11 accounts: it is a realm unit test.
name   chat-limits
about  5 lines per 10 s across rooms and whispers, 2 Global lines per 30 s, and the same text again in a room within 30 s are RateLimited.
rule   LINES    Say and Whisper together: 5 lines per 10 s (contract 9)
rule   GLOBAL   Say in Global: 2 lines per 30 s (contract 9)
rule   REPEAT   the same text again in the same room within 30 s is refused (contract 9)
bots   3

all connect
b0  register jon jon-pass-1               expect ok
b1  register kay kay-pass-1               expect ok
b2  register lea lea-pass-1               expect ok
b0  char-create Jonchar 0                 expect ok
b1  char-create Kaychar 1                 expect ok
b2  char-create Leachar 2                 expect ok
b0  char-select Jonchar                   expect ok
b1  char-select Kaychar                   expect ok
b2  char-select Leachar                   expect ok
b0  say mod line-one                      expect ok                                  rule LINES
b0  say mod line-two                      expect ok                                  rule LINES
b0  say global line-three                 expect ok                                  rule LINES
b0  whisper Kaychar line-four             expect ok                                  rule LINES
b0  say mod line-five                     expect ok                                  rule LINES
b0  say mod line-six                      expect refused=RateLimited wait_s>=1       rule LINES
b0  whisper Kaychar line-six              expect refused=RateLimited                 rule LINES
b1  say global first                      expect ok                                  rule GLOBAL
b1  say global second                     expect ok                                  rule GLOBAL
b1  say global third                      expect refused=RateLimited wait_s>=1       rule GLOBAL
b1  say mod still-fine-in-mod             expect ok                                  rule GLOBAL
b2  say mod wts-ring                      expect ok                                  rule REPEAT
b2  say mod wts-ring                      expect refused=RateLimited                 rule REPEAT
b2  say global wts-ring                   expect ok                                  rule REPEAT
