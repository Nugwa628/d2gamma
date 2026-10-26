# Join a friend's game (lobby contract 5): the friends list gives a mutual
# friend's game with its id; the client joins it with the usual JoinGame
# and its rules (a locked game needs the password).
name   friends-join
about  b0 and b1 are mutual friends, b2 only added b1: b1 creates a locked game; b0 joins it from the friend row's game id (refused without the password and with a wrong one, in with it); b2's row shows no game to join.
rule   JOIN     a mutual friend's row carries the game's id and name, and joining it by that id follows the join rules (contract 5)
rule   LOCKED   a locked friend's game still needs its password (contract 5)
rule   ONESIDED a one-sided friend's row has no game (contract 5)
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
b0  friend-add Jonchar                    expect ok                                  rule JOIN
b1  friend-add Ivychar                    expect ok                                  rule JOIN
b2  friend-add Jonchar                    expect ok                                  rule ONESIDED
b1  create-game lockgame password=s3cret  expect ok                                  rule JOIN
b0  friend-list Jonchar                   expect Jonchar.where=game Jonchar.game=lockgame Jonchar.joinable=0   within 5s   rule JOIN
b0  friend-join Jonchar                   expect refused=PasswordNeeded              rule LOCKED
b0  friend-join Jonchar password=wrong    expect refused=WrongPassword               rule LOCKED
b0  friend-join Jonchar password=s3cret   expect ok                                  rule LOCKED
b2  friend-list Jonchar                   expect Jonchar.where=hidden Jonchar.game_id=0   within 3s   rule ONESIDED
b2  friend-join Jonchar                   expect error                               rule ONESIDED
