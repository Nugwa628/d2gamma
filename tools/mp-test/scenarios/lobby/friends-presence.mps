# Friends and where they are (lobby contract 5): friends are one-sided, and
# only mutual friends see the place (the lobby, the game's name); everyone
# on the list sees online or offline and when the friend was last seen.
name   friends-presence
about  b0 adds b1: one-sided, b0 sees b1 online only; b1 adds b0 back: mutual, b0 sees the lobby, then the game after b1 creates one, then the last seen time after b1 logs out; the notices follow.
rule   ONESIDED  a friend who didn't add you back shows online or offline only (contract 5)
rule   MUTUAL    mutual friends see the place: the lobby, the game's name (contract 5)
rule   NOTICE    the accounts that added a friend hear it come online, enter and leave a game, go offline (contract 5)
rule   SEEN      a friend who logged out shows when it was last seen (contract 5)
bots   2
server limits=test

all connect
b0  register ivy ivy-pass-1               expect ok
b1  register jon jon-pass-1               expect ok
b0  char-create Ivychar 0                 expect ok
b1  char-create Jonchar 1                 expect ok
b0  char-select Ivychar                   expect ok
b1  char-select Jonchar                   expect ok
b0  friend-add Jonchar                    expect ok                                  rule ONESIDED
b0  friend-list Jonchar                   expect has.Jonchar=yes Jonchar.where=hidden Jonchar.mutual=no Jonchar.seen=no   within 3s   rule ONESIDED
b1  create-game jonwarm                   expect ok                                  rule ONESIDED
b0  friend-list Jonchar                   expect Jonchar.where=hidden Jonchar.game_id=0 notices=0   within 3s   rule ONESIDED
b1  leave-game
b1  games                                 expect has.jonwarm=no                      within 10s   rule ONESIDED
b1  friend-add *ivy                       expect ok                                  rule MUTUAL
b0  friend-list Jonchar                   expect Jonchar.where=lobby Jonchar.mutual=yes notice=added notice.from=Jonchar   within 3s   rule MUTUAL
b1  create-game jongame                   expect ok                                  rule MUTUAL
b0  friend-list Jonchar                   expect Jonchar.where=game Jonchar.game=jongame notice=entered   within 5s   rule MUTUAL
b1  leave-game
b0  friend-list Jonchar                   expect Jonchar.where=lobby notice=left     within 10s  rule NOTICE
b1  logout                                expect ok
b0  friend-list Jonchar                   expect Jonchar.where=offline Jonchar.seen=yes notice=offline   within 5s   rule SEEN
b1  connect                               expect ok
b1  login jon jon-pass-1                  expect ok
b0  friend-list Jonchar                   expect Jonchar.where=lobby notice=online   within 5s   rule NOTICE
