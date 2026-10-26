# Privacy (lobby contract 3.4, 5): show_where "nobody" hides the place
# from mutual friends too; whispers "friends" takes DMs only from the
# accounts one added (and the guild), "nobody" from no one.
name   friends-privacy
about  b0 and b1 are mutual friends, b2 a stranger: b1 hides where it is (b0 sees hidden, also in a game) and shows it again (b0 sees the game); b1 takes whispers from friends only (b2 is WhisperBlocked, b0 gets through), then from nobody (b0 is blocked too).
rule   WHERE    show_where 1 shows the player as hidden to every friend (contract 3.4)
rule   WHISPER  whispers 1 blocks a stranger and lets a friend through; 2 blocks everyone (contract 3.4)
bots   3
server limits=test

all connect
b0  register ivy ivy-pass-1               expect ok
b1  register jon jon-pass-1               expect ok
b2  register kim kim-pass-1               expect ok
b0  char-create Ivychar 0                 expect ok
b1  char-create Jonchar 1                 expect ok
b2  char-create Kimchar 2                 expect ok
b0  char-select Ivychar                   expect ok
b1  char-select Jonchar                   expect ok
b2  char-select Kimchar                   expect ok
b0  friend-add Jonchar                    expect ok                                  rule WHERE
b1  friend-add Ivychar                    expect ok                                  rule WHERE
b0  friend-list Jonchar                   expect Jonchar.where=lobby Jonchar.mutual=yes   within 3s   rule WHERE
b1  friend-privacy where=nobody           expect ok                                  rule WHERE
b0  friend-list Jonchar                   expect Jonchar.where=hidden Jonchar.mutual=yes   within 3s   rule WHERE
b1  create-game hidegame                  expect ok                                  rule WHERE
b0  friend-list Jonchar                   expect Jonchar.where=hidden Jonchar.game_id=0   within 3s   rule WHERE
b1  friend-privacy where=friends whispers=friends   expect ok                        rule WHISPER
b0  friend-list Jonchar                   expect Jonchar.where=game Jonchar.game=hidegame   within 5s   rule WHERE
b2  whisper Jonchar hello                 expect refused=WhisperBlocked              rule WHISPER
b0  whisper Jonchar hello                 expect ok                                  rule WHISPER
b1  friend-privacy whispers=nobody        expect ok                                  rule WHISPER
b0  whisper Jonchar again                 expect refused=WhisperBlocked              rule WHISPER
b2  whisper Jonchar again                 expect refused=WhisperBlocked              rule WHISPER
