# Game list v2 (owner 2026-10-23, lobby contract section 7): a listed game carries its server
# (the game server's name and its NodeList id; player-hosted marked), the creator's
# description and level range, and when it was made. CreateGame refuses a bad range.
name   lobby-games-fields
about  The description, level range, server and creation time a creator's game gets reach another bot's list; a range that leaves out the creator's level or has its min above its max is refused.
rule   FIELDS  a listed game carries its description, level range, server label and node id, not player-hosted on an official server, and its creation time (lobby contract 7)
rule   CREATE  CreateGame refuses a level range the creator's level is outside of or whose min is above its max (LevelRange) (lobby contract 7; a description over 40 bytes is refused by the codec and the unit tests)
bots   2

all connect
b0  register fiona  fiona-pass-1          expect ok
b1  register gus    gus-pass-1            expect ok
b0  char-create Fionaama 0                expect ok
b1  char-create Gussor 1                  expect ok
b0  char-select Fionaama                  expect ok
b1  char-select Gussor                    expect ok
b0  create-game toohigh levels=10-20      expect refused=LevelRange                       rule CREATE
b0  create-game backward levels=9-3       expect refused=LevelRange                       rule CREATE
b0  create-game baalrun desc=baal-runs-fast levels=1-10   expect ok                       rule CREATE
b1  games                                 expect has.baalrun=yes baalrun.desc=baal-runs-fast baalrun.levels=1-10 baalrun.joinable=0   within 3s   rule FIELDS
b1  games                                 expect baalrun.server=n1 baalrun.node=n1 baalrun.hosted=no baalrun.created=yes   within 3s   rule FIELDS
b1  join-game baalrun                     expect ok                                       rule FIELDS
