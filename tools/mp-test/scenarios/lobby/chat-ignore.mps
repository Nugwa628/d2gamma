# Ignore is applied by the realm (lobby contract 3.1, 3.2): an account
# never gets room lines or DMs from the accounts it ignores (`friend-ignore`,
# lobby contract 5).
name   chat-ignore
about  b1 ignores b0: b0's room lines don't reach b1 and b0's whisper is WhisperBlocked; b2 still hears b0.
rule   ROOM     an ignored account's room lines don't reach the ignoring account (contract 3.1)
rule   DM       a whisper to an account that ignores the sender is WhisperBlocked (contract 3.2)
bots   3

all connect
b0  register mia mia-pass-1               expect ok
b1  register ned ned-pass-1               expect ok
b2  register ola ola-pass-1               expect ok
b0  char-create Miachar 3                 expect ok
b1  char-create Nedchar 4                 expect ok
b2  char-create Olachar 5                 expect ok
b0  char-select Miachar                   expect ok
b1  char-select Nedchar                   expect ok
b2  char-select Olachar                   expect ok
b1  heard-in mod                          expect ok                                  rule ROOM
b1  friend-ignore Miachar                 expect ok                                  rule ROOM
b2  say mod ola-here                      expect ok                                  rule ROOM
b0  say mod can-you-hear-me               expect ok                                  rule ROOM
b2  heard-in mod                          expect said=can-you-hear-me                within 3s   rule ROOM
b1  heard-in mod                          expect said=ola-here                       within 3s   rule ROOM
b0  whisper Nedchar hello                 expect refused=WhisperBlocked              rule DM
b0  whisper Olachar hello                 expect ok                                  rule DM
