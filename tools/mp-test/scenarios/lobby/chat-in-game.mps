# The rooms and DMs follow a session into a game (lobby contract 3.1): a
# player in a game still hears Global and gets DMs, and shows as in a game.
name   chat-in-game
about  b1 enters a game and still hears Global and gets a DM; the user list shows it in a game.
rule   ROOMS    a session is in its rooms in the lobby and in a game alike (contract 3.1)
rule   DM       a DM reaches a player in a game at once (contract 3.2)
rule   WHERE    the user list shows a player in a game as in a game (contract 2, 3)
bots   2

all connect
b0  register hana hana-pass-1             expect ok
b1  register ivo ivo-pass-1               expect ok
b0  char-create Hanachar 1                expect ok
b1  char-create Ivochar 2                 expect ok
b0  char-select Hanachar                  expect ok
b1  char-select Ivochar                   expect ok
b1  heard-in global                       expect ok                                  rule ROOMS
b1  create-game chat-game-1               expect ok                                  rule ROOMS
b0  say global anyone-in-a-game           expect ok                                  rule ROOMS
b1  heard-in global                       expect said=anyone-in-a-game said.from=Hanachar   within 3s   rule ROOMS
b0  whisper Ivochar dm-in-game            expect ok stored=no                        rule DM
b1  dm-unread                             expect pushed=1 hana.unread=1              within 3s   rule DM
b0  users mod Ivochar Hanachar            expect count=2 Ivochar.where=game Hanachar.where=lobby   within 3s   rule WHERE
