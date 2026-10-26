# Players moving between the lobby and games for ten minutes (lobby plan
# L17, contract 3 and 5): friends' places and the room's user lists stay
# right through the churn. Written by tools/mp-test/gen_lobby_load.py:
# change that, not this file.
name   soak-presence
about  18 bots create, join and leave games for 20 rounds (about 10 minutes) while 6 stay in the lobby: their mutual friends' places, a watched user list kept by pushes and a fresh one match where every bot is; at the end the bots log out and the lists empty.
rule   FRIENDS  a mutual friend's place follows it into a game and back to the lobby (contract 5)
rule   WATCHED  a watched user list kept by RoomPresence pushes stays the room's members and places (contract 3)
rule   FRESH    a user list asked again matches it (contract 3)
rule   GONE     players who logged out leave the lists and show offline (contract 3, 5)
bots   24
server limits=test
server max_games=32

all connect
b0  register spaa spaa-pass-1             expect ok
b1  register spab spab-pass-1             expect ok
b2  register spac spac-pass-1             expect ok
b3  register spad spad-pass-1             expect ok
b4  register spae spae-pass-1             expect ok
b5  register spaf spaf-pass-1             expect ok
b6  register spag spag-pass-1             expect ok
b7  register spah spah-pass-1             expect ok
b8  register spai spai-pass-1             expect ok
b9  register spaj spaj-pass-1             expect ok
b10 register spak spak-pass-1             expect ok
b11 register spal spal-pass-1             expect ok
b12 register spam spam-pass-1             expect ok
b13 register span span-pass-1             expect ok
b14 register spao spao-pass-1             expect ok
b15 register spap spap-pass-1             expect ok
b16 register spaq spaq-pass-1             expect ok
b17 register spar spar-pass-1             expect ok
b18 register spas spas-pass-1             expect ok
b19 register spat spat-pass-1             expect ok
b20 register spau spau-pass-1             expect ok
b21 register spav spav-pass-1             expect ok
b22 register spaw spaw-pass-1             expect ok
b23 register spax spax-pass-1             expect ok
b0  char-create Spaachar 0                expect ok
b1  char-create Spabchar 1                expect ok
b2  char-create Spacchar 2                expect ok
b3  char-create Spadchar 3                expect ok
b4  char-create Spaechar 4                expect ok
b5  char-create Spafchar 5                expect ok
b6  char-create Spagchar 6                expect ok
b7  char-create Spahchar 0                expect ok
b8  char-create Spaichar 1                expect ok
b9  char-create Spajchar 2                expect ok
b10 char-create Spakchar 3                expect ok
b11 char-create Spalchar 4                expect ok
b12 char-create Spamchar 5                expect ok
b13 char-create Spanchar 6                expect ok
b14 char-create Spaochar 0                expect ok
b15 char-create Spapchar 1                expect ok
b16 char-create Spaqchar 2                expect ok
b17 char-create Sparchar 3                expect ok
b18 char-create Spaschar 4                expect ok
b19 char-create Spatchar 5                expect ok
b20 char-create Spauchar 6                expect ok
b21 char-create Spavchar 0                expect ok
b22 char-create Spawchar 1                expect ok
b23 char-create Spaxchar 2                expect ok
b0  char-select Spaachar                  expect ok
b1  char-select Spabchar                  expect ok
b2  char-select Spacchar                  expect ok
b3  char-select Spadchar                  expect ok
b4  char-select Spaechar                  expect ok
b5  char-select Spafchar                  expect ok
b6  char-select Spagchar                  expect ok
b7  char-select Spahchar                  expect ok
b8  char-select Spaichar                  expect ok
b9  char-select Spajchar                  expect ok
b10 char-select Spakchar                  expect ok
b11 char-select Spalchar                  expect ok
b12 char-select Spamchar                  expect ok
b13 char-select Spanchar                  expect ok
b14 char-select Spaochar                  expect ok
b15 char-select Spapchar                  expect ok
b16 char-select Spaqchar                  expect ok
b17 char-select Sparchar                  expect ok
b18 char-select Spaschar                  expect ok
b19 char-select Spatchar                  expect ok
b20 char-select Spauchar                  expect ok
b21 char-select Spavchar                  expect ok
b22 char-select Spawchar                  expect ok
b23 char-select Spaxchar                  expect ok
b0  heard-in global                       expect ok
b1  heard-in global                       expect ok
b2  heard-in global                       expect ok
b3  heard-in global                       expect ok
b4  heard-in global                       expect ok
b5  heard-in global                       expect ok
b6  heard-in global                       expect ok
b7  heard-in global                       expect ok
b8  heard-in global                       expect ok
b9  heard-in global                       expect ok
b10 heard-in global                       expect ok
b11 heard-in global                       expect ok
b12 heard-in global                       expect ok
b13 heard-in global                       expect ok
b14 heard-in global                       expect ok
b15 heard-in global                       expect ok
b16 heard-in global                       expect ok
b17 heard-in global                       expect ok
b18 heard-in global                       expect ok
b19 heard-in global                       expect ok
b20 heard-in global                       expect ok
b21 heard-in global                       expect ok
b22 heard-in global                       expect ok
b23 heard-in global                       expect ok

# Mutual friends: every mover and its watcher.
b0  friend-add Spagchar                   expect ok
b6  friend-add Spaachar                   expect ok
b0  friend-add Spamchar                   expect ok
b12 friend-add Spaachar                   expect ok
b0  friend-add Spaschar                   expect ok
b18 friend-add Spaachar                   expect ok
b1  friend-add Spahchar                   expect ok
b7  friend-add Spabchar                   expect ok
b1  friend-add Spanchar                   expect ok
b13 friend-add Spabchar                   expect ok
b1  friend-add Spatchar                   expect ok
b19 friend-add Spabchar                   expect ok
b2  friend-add Spaichar                   expect ok
b8  friend-add Spacchar                   expect ok
b2  friend-add Spaochar                   expect ok
b14 friend-add Spacchar                   expect ok
b2  friend-add Spauchar                   expect ok
b20 friend-add Spacchar                   expect ok
b3  friend-add Spajchar                   expect ok
b9  friend-add Spadchar                   expect ok
b3  friend-add Spapchar                   expect ok
b15 friend-add Spadchar                   expect ok
b3  friend-add Spavchar                   expect ok
b21 friend-add Spadchar                   expect ok
b4  friend-add Spakchar                   expect ok
b10 friend-add Spaechar                   expect ok
b4  friend-add Spaqchar                   expect ok
b16 friend-add Spaechar                   expect ok
b4  friend-add Spawchar                   expect ok
b22 friend-add Spaechar                   expect ok
b5  friend-add Spalchar                   expect ok
b11 friend-add Spafchar                   expect ok
b5  friend-add Sparchar                   expect ok
b17 friend-add Spafchar                   expect ok
b5  friend-add Spaxchar                   expect ok
b23 friend-add Spafchar                   expect ok
b0  users global watch=yes                expect count=24   within 3s   rule WATCHED

# Round 1: a third create games, a third join them, a third stay.
b6  create-game spagaa                    expect ok
b9  create-game spajaa                    expect ok
b12 create-game spamaa                    expect ok
b15 create-game spapaa                    expect ok
b18 create-game spasaa                    expect ok
b21 create-game spavaa                    expect ok
b7  join-game spagaa                      expect ok
b10 join-game spajaa                      expect ok
b13 join-game spamaa                      expect ok
b16 join-game spapaa                      expect ok
b19 join-game spasaa                      expect ok
b22 join-game spavaa                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=game Spagchar.game=spagaa   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spamaa   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=spasaa   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spagaa   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spamaa   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spasaa   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spajaa   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spapaa   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spavaa   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spajaa   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spapaa   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spavaa   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule FRESH
b6  leave-game
b7  leave-game
b9  leave-game
b10 leave-game
b12 leave-game
b13 leave-game
b15 leave-game
b16 leave-game
b18 leave-game
b19 leave-game
b21 leave-game
b22 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 2: a third create games, a third join them, a third stay.
wait 20s
b8  create-game spaiab                    expect ok
b11 create-game spalab                    expect ok
b14 create-game spaoab                    expect ok
b17 create-game sparab                    expect ok
b20 create-game spauab                    expect ok
b23 create-game spaxab                    expect ok
b9  join-game spaiab                      expect ok
b12 join-game spalab                      expect ok
b15 join-game spaoab                      expect ok
b18 join-game sparab                      expect ok
b21 join-game spauab                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spalab   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=sparab   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spaiab   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spaoab   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spauab   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spaiab   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spaoab   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spauab   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spalab   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=sparab   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spaxab   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule FRESH
b8  leave-game
b9  leave-game
b11 leave-game
b12 leave-game
b14 leave-game
b15 leave-game
b17 leave-game
b18 leave-game
b20 leave-game
b21 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 3: a third create games, a third join them, a third stay.
wait 20s
b7  create-game spahac                    expect ok
b10 create-game spakac                    expect ok
b13 create-game spanac                    expect ok
b16 create-game spaqac                    expect ok
b19 create-game spatac                    expect ok
b22 create-game spawac                    expect ok
b8  join-game spahac                      expect ok
b11 join-game spakac                      expect ok
b14 join-game spanac                      expect ok
b17 join-game spaqac                      expect ok
b20 join-game spatac                      expect ok
b23 join-game spawac                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spahac   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spanac   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spatac   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spahac   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spanac   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spatac   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spakac   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spaqac   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spawac   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spakac   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=spaqac   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spawac   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule FRESH
b7  leave-game
b8  leave-game
b10 leave-game
b11 leave-game
b13 leave-game
b14 leave-game
b16 leave-game
b17 leave-game
b19 leave-game
b20 leave-game
b22 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 4: a third create games, a third join them, a third stay.
wait 20s
b6  create-game spagad                    expect ok
b9  create-game spajad                    expect ok
b12 create-game spamad                    expect ok
b15 create-game spapad                    expect ok
b18 create-game spasad                    expect ok
b21 create-game spavad                    expect ok
b7  join-game spagad                      expect ok
b10 join-game spajad                      expect ok
b13 join-game spamad                      expect ok
b16 join-game spapad                      expect ok
b19 join-game spasad                      expect ok
b22 join-game spavad                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=game Spagchar.game=spagad   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spamad   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=spasad   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spagad   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spamad   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spasad   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spajad   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spapad   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spavad   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spajad   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spapad   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spavad   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule FRESH
b6  leave-game
b7  leave-game
b9  leave-game
b10 leave-game
b12 leave-game
b13 leave-game
b15 leave-game
b16 leave-game
b18 leave-game
b19 leave-game
b21 leave-game
b22 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 5: a third create games, a third join them, a third stay.
wait 20s
b8  create-game spaiae                    expect ok
b11 create-game spalae                    expect ok
b14 create-game spaoae                    expect ok
b17 create-game sparae                    expect ok
b20 create-game spauae                    expect ok
b23 create-game spaxae                    expect ok
b9  join-game spaiae                      expect ok
b12 join-game spalae                      expect ok
b15 join-game spaoae                      expect ok
b18 join-game sparae                      expect ok
b21 join-game spauae                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spalae   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=sparae   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spaiae   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spaoae   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spauae   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spaiae   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spaoae   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spauae   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spalae   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=sparae   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spaxae   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule FRESH
b8  leave-game
b9  leave-game
b11 leave-game
b12 leave-game
b14 leave-game
b15 leave-game
b17 leave-game
b18 leave-game
b20 leave-game
b21 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 6: a third create games, a third join them, a third stay.
wait 20s
b7  create-game spahaf                    expect ok
b10 create-game spakaf                    expect ok
b13 create-game spanaf                    expect ok
b16 create-game spaqaf                    expect ok
b19 create-game spataf                    expect ok
b22 create-game spawaf                    expect ok
b8  join-game spahaf                      expect ok
b11 join-game spakaf                      expect ok
b14 join-game spanaf                      expect ok
b17 join-game spaqaf                      expect ok
b20 join-game spataf                      expect ok
b23 join-game spawaf                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spahaf   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spanaf   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spataf   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spahaf   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spanaf   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spataf   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spakaf   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spaqaf   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spawaf   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spakaf   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=spaqaf   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spawaf   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule FRESH
b7  leave-game
b8  leave-game
b10 leave-game
b11 leave-game
b13 leave-game
b14 leave-game
b16 leave-game
b17 leave-game
b19 leave-game
b20 leave-game
b22 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 7: a third create games, a third join them, a third stay.
wait 20s
b6  create-game spagag                    expect ok
b9  create-game spajag                    expect ok
b12 create-game spamag                    expect ok
b15 create-game spapag                    expect ok
b18 create-game spasag                    expect ok
b21 create-game spavag                    expect ok
b7  join-game spagag                      expect ok
b10 join-game spajag                      expect ok
b13 join-game spamag                      expect ok
b16 join-game spapag                      expect ok
b19 join-game spasag                      expect ok
b22 join-game spavag                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=game Spagchar.game=spagag   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spamag   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=spasag   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spagag   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spamag   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spasag   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spajag   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spapag   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spavag   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spajag   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spapag   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spavag   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule FRESH
b6  leave-game
b7  leave-game
b9  leave-game
b10 leave-game
b12 leave-game
b13 leave-game
b15 leave-game
b16 leave-game
b18 leave-game
b19 leave-game
b21 leave-game
b22 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 8: a third create games, a third join them, a third stay.
wait 20s
b8  create-game spaiah                    expect ok
b11 create-game spalah                    expect ok
b14 create-game spaoah                    expect ok
b17 create-game sparah                    expect ok
b20 create-game spauah                    expect ok
b23 create-game spaxah                    expect ok
b9  join-game spaiah                      expect ok
b12 join-game spalah                      expect ok
b15 join-game spaoah                      expect ok
b18 join-game sparah                      expect ok
b21 join-game spauah                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spalah   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=sparah   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spaiah   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spaoah   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spauah   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spaiah   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spaoah   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spauah   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spalah   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=sparah   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spaxah   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule FRESH
b8  leave-game
b9  leave-game
b11 leave-game
b12 leave-game
b14 leave-game
b15 leave-game
b17 leave-game
b18 leave-game
b20 leave-game
b21 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 9: a third create games, a third join them, a third stay.
wait 20s
b7  create-game spahai                    expect ok
b10 create-game spakai                    expect ok
b13 create-game spanai                    expect ok
b16 create-game spaqai                    expect ok
b19 create-game spatai                    expect ok
b22 create-game spawai                    expect ok
b8  join-game spahai                      expect ok
b11 join-game spakai                      expect ok
b14 join-game spanai                      expect ok
b17 join-game spaqai                      expect ok
b20 join-game spatai                      expect ok
b23 join-game spawai                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spahai   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spanai   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spatai   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spahai   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spanai   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spatai   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spakai   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spaqai   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spawai   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spakai   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=spaqai   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spawai   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule FRESH
b7  leave-game
b8  leave-game
b10 leave-game
b11 leave-game
b13 leave-game
b14 leave-game
b16 leave-game
b17 leave-game
b19 leave-game
b20 leave-game
b22 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 10: a third create games, a third join them, a third stay.
wait 20s
b6  create-game spagaj                    expect ok
b9  create-game spajaj                    expect ok
b12 create-game spamaj                    expect ok
b15 create-game spapaj                    expect ok
b18 create-game spasaj                    expect ok
b21 create-game spavaj                    expect ok
b7  join-game spagaj                      expect ok
b10 join-game spajaj                      expect ok
b13 join-game spamaj                      expect ok
b16 join-game spapaj                      expect ok
b19 join-game spasaj                      expect ok
b22 join-game spavaj                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=game Spagchar.game=spagaj   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spamaj   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=spasaj   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spagaj   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spamaj   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spasaj   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spajaj   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spapaj   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spavaj   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spajaj   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spapaj   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spavaj   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule FRESH
b6  leave-game
b7  leave-game
b9  leave-game
b10 leave-game
b12 leave-game
b13 leave-game
b15 leave-game
b16 leave-game
b18 leave-game
b19 leave-game
b21 leave-game
b22 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 11: a third create games, a third join them, a third stay.
wait 20s
b8  create-game spaiak                    expect ok
b11 create-game spalak                    expect ok
b14 create-game spaoak                    expect ok
b17 create-game sparak                    expect ok
b20 create-game spauak                    expect ok
b23 create-game spaxak                    expect ok
b9  join-game spaiak                      expect ok
b12 join-game spalak                      expect ok
b15 join-game spaoak                      expect ok
b18 join-game sparak                      expect ok
b21 join-game spauak                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spalak   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=sparak   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spaiak   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spaoak   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spauak   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spaiak   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spaoak   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spauak   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spalak   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=sparak   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spaxak   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule FRESH
b8  leave-game
b9  leave-game
b11 leave-game
b12 leave-game
b14 leave-game
b15 leave-game
b17 leave-game
b18 leave-game
b20 leave-game
b21 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 12: a third create games, a third join them, a third stay.
wait 20s
b7  create-game spahal                    expect ok
b10 create-game spakal                    expect ok
b13 create-game spanal                    expect ok
b16 create-game spaqal                    expect ok
b19 create-game spatal                    expect ok
b22 create-game spawal                    expect ok
b8  join-game spahal                      expect ok
b11 join-game spakal                      expect ok
b14 join-game spanal                      expect ok
b17 join-game spaqal                      expect ok
b20 join-game spatal                      expect ok
b23 join-game spawal                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spahal   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spanal   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spatal   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spahal   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spanal   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spatal   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spakal   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spaqal   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spawal   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spakal   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=spaqal   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spawal   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule FRESH
b7  leave-game
b8  leave-game
b10 leave-game
b11 leave-game
b13 leave-game
b14 leave-game
b16 leave-game
b17 leave-game
b19 leave-game
b20 leave-game
b22 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 13: a third create games, a third join them, a third stay.
wait 20s
b6  create-game spagam                    expect ok
b9  create-game spajam                    expect ok
b12 create-game spamam                    expect ok
b15 create-game spapam                    expect ok
b18 create-game spasam                    expect ok
b21 create-game spavam                    expect ok
b7  join-game spagam                      expect ok
b10 join-game spajam                      expect ok
b13 join-game spamam                      expect ok
b16 join-game spapam                      expect ok
b19 join-game spasam                      expect ok
b22 join-game spavam                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=game Spagchar.game=spagam   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spamam   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=spasam   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spagam   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spamam   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spasam   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spajam   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spapam   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spavam   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spajam   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spapam   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spavam   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule FRESH
b6  leave-game
b7  leave-game
b9  leave-game
b10 leave-game
b12 leave-game
b13 leave-game
b15 leave-game
b16 leave-game
b18 leave-game
b19 leave-game
b21 leave-game
b22 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 14: a third create games, a third join them, a third stay.
wait 20s
b8  create-game spaian                    expect ok
b11 create-game spalan                    expect ok
b14 create-game spaoan                    expect ok
b17 create-game sparan                    expect ok
b20 create-game spauan                    expect ok
b23 create-game spaxan                    expect ok
b9  join-game spaian                      expect ok
b12 join-game spalan                      expect ok
b15 join-game spaoan                      expect ok
b18 join-game sparan                      expect ok
b21 join-game spauan                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spalan   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=sparan   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spaian   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spaoan   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spauan   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spaian   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spaoan   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spauan   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spalan   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=sparan   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spaxan   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule FRESH
b8  leave-game
b9  leave-game
b11 leave-game
b12 leave-game
b14 leave-game
b15 leave-game
b17 leave-game
b18 leave-game
b20 leave-game
b21 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 15: a third create games, a third join them, a third stay.
wait 20s
b7  create-game spahao                    expect ok
b10 create-game spakao                    expect ok
b13 create-game spanao                    expect ok
b16 create-game spaqao                    expect ok
b19 create-game spatao                    expect ok
b22 create-game spawao                    expect ok
b8  join-game spahao                      expect ok
b11 join-game spakao                      expect ok
b14 join-game spanao                      expect ok
b17 join-game spaqao                      expect ok
b20 join-game spatao                      expect ok
b23 join-game spawao                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spahao   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spanao   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spatao   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spahao   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spanao   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spatao   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spakao   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spaqao   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spawao   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spakao   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=spaqao   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spawao   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule FRESH
b7  leave-game
b8  leave-game
b10 leave-game
b11 leave-game
b13 leave-game
b14 leave-game
b16 leave-game
b17 leave-game
b19 leave-game
b20 leave-game
b22 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 16: a third create games, a third join them, a third stay.
wait 20s
b6  create-game spagap                    expect ok
b9  create-game spajap                    expect ok
b12 create-game spamap                    expect ok
b15 create-game spapap                    expect ok
b18 create-game spasap                    expect ok
b21 create-game spavap                    expect ok
b7  join-game spagap                      expect ok
b10 join-game spajap                      expect ok
b13 join-game spamap                      expect ok
b16 join-game spapap                      expect ok
b19 join-game spasap                      expect ok
b22 join-game spavap                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=game Spagchar.game=spagap   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spamap   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=spasap   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spagap   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spamap   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spasap   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spajap   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spapap   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spavap   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spajap   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spapap   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spavap   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule FRESH
b6  leave-game
b7  leave-game
b9  leave-game
b10 leave-game
b12 leave-game
b13 leave-game
b15 leave-game
b16 leave-game
b18 leave-game
b19 leave-game
b21 leave-game
b22 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 17: a third create games, a third join them, a third stay.
wait 20s
b8  create-game spaiaq                    expect ok
b11 create-game spalaq                    expect ok
b14 create-game spaoaq                    expect ok
b17 create-game sparaq                    expect ok
b20 create-game spauaq                    expect ok
b23 create-game spaxaq                    expect ok
b9  join-game spaiaq                      expect ok
b12 join-game spalaq                      expect ok
b15 join-game spaoaq                      expect ok
b18 join-game sparaq                      expect ok
b21 join-game spauaq                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spalaq   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=sparaq   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spaiaq   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spaoaq   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spauaq   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spaiaq   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spaoaq   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spauaq   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spalaq   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=sparaq   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spaxaq   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule FRESH
b8  leave-game
b9  leave-game
b11 leave-game
b12 leave-game
b14 leave-game
b15 leave-game
b17 leave-game
b18 leave-game
b20 leave-game
b21 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 18: a third create games, a third join them, a third stay.
wait 20s
b7  create-game spahar                    expect ok
b10 create-game spakar                    expect ok
b13 create-game spanar                    expect ok
b16 create-game spaqar                    expect ok
b19 create-game spatar                    expect ok
b22 create-game spawar                    expect ok
b8  join-game spahar                      expect ok
b11 join-game spakar                      expect ok
b14 join-game spanar                      expect ok
b17 join-game spaqar                      expect ok
b20 join-game spatar                      expect ok
b23 join-game spawar                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spahar   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spanar   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spatar   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spahar   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spanar   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spatar   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spakar   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spaqar   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spawar   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spakar   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=spaqar   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spawar   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=game Spaichar.where=game Spajchar.where=lobby Spakchar.where=game Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=game Spaochar.where=game Spapchar.where=lobby Spaqchar.where=game Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=game Spauchar.where=game Spavchar.where=lobby Spawchar.where=game Spaxchar.where=game   within 10s   rule FRESH
b7  leave-game
b8  leave-game
b10 leave-game
b11 leave-game
b13 leave-game
b14 leave-game
b16 leave-game
b17 leave-game
b19 leave-game
b20 leave-game
b22 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 19: a third create games, a third join them, a third stay.
wait 20s
b6  create-game spagas                    expect ok
b9  create-game spajas                    expect ok
b12 create-game spamas                    expect ok
b15 create-game spapas                    expect ok
b18 create-game spasas                    expect ok
b21 create-game spavas                    expect ok
b7  join-game spagas                      expect ok
b10 join-game spajas                      expect ok
b13 join-game spamas                      expect ok
b16 join-game spapas                      expect ok
b19 join-game spasas                      expect ok
b22 join-game spavas                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=game Spagchar.game=spagas   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spamas   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=spasas   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=game Spahchar.game=spagas   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=game Spanchar.game=spamas   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=game Spatchar.game=spasas   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spajas   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spapas   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spavas   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=game Spakchar.game=spajas   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=game Spaqchar.game=spapas   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=game Spawchar.game=spavas   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=game Spahchar.where=game Spaichar.where=lobby Spajchar.where=game Spakchar.where=game Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=game Spaochar.where=lobby Spapchar.where=game Spaqchar.where=game Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=game Spauchar.where=lobby Spavchar.where=game Spawchar.where=game Spaxchar.where=lobby   within 10s   rule FRESH
b6  leave-game
b7  leave-game
b9  leave-game
b10 leave-game
b12 leave-game
b13 leave-game
b15 leave-game
b16 leave-game
b18 leave-game
b19 leave-game
b21 leave-game
b22 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# Round 20: a third create games, a third join them, a third stay.
wait 20s
b8  create-game spaiat                    expect ok
b11 create-game spalat                    expect ok
b14 create-game spaoat                    expect ok
b17 create-game sparat                    expect ok
b20 create-game spauat                    expect ok
b23 create-game spaxat                    expect ok
b9  join-game spaiat                      expect ok
b12 join-game spalat                      expect ok
b15 join-game spaoat                      expect ok
b18 join-game sparat                      expect ok
b21 join-game spauat                      expect ok
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=game Spamchar.game=spalat   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=game Spaschar.game=sparat   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=game Spaichar.game=spaiat   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=game Spaochar.game=spaoat   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=game Spauchar.game=spauat   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=game Spajchar.game=spaiat   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=game Spapchar.game=spaoat   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=game Spavchar.game=spauat   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=game Spalchar.game=spalat   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=game Sparchar.game=sparat   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=game Spaxchar.game=spaxat   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=game Spajchar.where=game Spakchar.where=lobby Spalchar.where=game   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=game Spanchar.where=lobby Spaochar.where=game Spapchar.where=game Spaqchar.where=lobby Sparchar.where=game   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=game Spatchar.where=lobby Spauchar.where=game Spavchar.where=game Spawchar.where=lobby Spaxchar.where=game   within 10s   rule FRESH
b8  leave-game
b9  leave-game
b11 leave-game
b12 leave-game
b14 leave-game
b15 leave-game
b17 leave-game
b18 leave-game
b20 leave-game
b21 leave-game
b23 leave-game
b0  friend-list Spagchar                  expect Spagchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spamchar                  expect Spamchar.where=lobby   within 10s   rule FRIENDS
b0  friend-list Spaschar                  expect Spaschar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spahchar                  expect Spahchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spanchar                  expect Spanchar.where=lobby   within 10s   rule FRIENDS
b1  friend-list Spatchar                  expect Spatchar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaichar                  expect Spaichar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spaochar                  expect Spaochar.where=lobby   within 10s   rule FRIENDS
b2  friend-list Spauchar                  expect Spauchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spajchar                  expect Spajchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spapchar                  expect Spapchar.where=lobby   within 10s   rule FRIENDS
b3  friend-list Spavchar                  expect Spavchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spakchar                  expect Spakchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spaqchar                  expect Spaqchar.where=lobby   within 10s   rule FRIENDS
b4  friend-list Spawchar                  expect Spawchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spalchar                  expect Spalchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Sparchar                  expect Sparchar.where=lobby   within 10s   rule FRIENDS
b5  friend-list Spaxchar                  expect Spaxchar.where=lobby   within 10s   rule FRIENDS
b0  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule WATCHED
b0  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule WATCHED
b0  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule WATCHED
b1  users global Spagchar Spahchar Spaichar Spajchar Spakchar Spalcharexpect Spagchar.where=lobby Spahchar.where=lobby Spaichar.where=lobby Spajchar.where=lobby Spakchar.where=lobby Spalchar.where=lobby   within 10s   rule FRESH
b1  users global Spamchar Spanchar Spaochar Spapchar Spaqchar Sparcharexpect Spamchar.where=lobby Spanchar.where=lobby Spaochar.where=lobby Spapchar.where=lobby Spaqchar.where=lobby Sparchar.where=lobby   within 10s   rule FRESH
b1  users global Spaschar Spatchar Spauchar Spavchar Spawchar Spaxcharexpect Spaschar.where=lobby Spatchar.where=lobby Spauchar.where=lobby Spavchar.where=lobby Spawchar.where=lobby Spaxchar.where=lobby   within 10s   rule FRESH

# The end: the movers log out.
b6  logout                                expect ok   rule GONE
b7  logout                                expect ok   rule GONE
b8  logout                                expect ok   rule GONE
b9  logout                                expect ok   rule GONE
b10 logout                                expect ok   rule GONE
b11 logout                                expect ok   rule GONE
b12 logout                                expect ok   rule GONE
b13 logout                                expect ok   rule GONE
b14 logout                                expect ok   rule GONE
b15 logout                                expect ok   rule GONE
b16 logout                                expect ok   rule GONE
b17 logout                                expect ok   rule GONE
b18 logout                                expect ok   rule GONE
b19 logout                                expect ok   rule GONE
b20 logout                                expect ok   rule GONE
b21 logout                                expect ok   rule GONE
b22 logout                                expect ok   rule GONE
b23 logout                                expect ok   rule GONE
b0  friend-list Spagchar                  expect Spagchar.where=offline   within 10s   rule GONE
b0  friend-list Spamchar                  expect Spamchar.where=offline   within 10s   rule GONE
b0  friend-list Spaschar                  expect Spaschar.where=offline   within 10s   rule GONE
b1  friend-list Spahchar                  expect Spahchar.where=offline   within 10s   rule GONE
b1  friend-list Spanchar                  expect Spanchar.where=offline   within 10s   rule GONE
b1  friend-list Spatchar                  expect Spatchar.where=offline   within 10s   rule GONE
b2  friend-list Spaichar                  expect Spaichar.where=offline   within 10s   rule GONE
b2  friend-list Spaochar                  expect Spaochar.where=offline   within 10s   rule GONE
b2  friend-list Spauchar                  expect Spauchar.where=offline   within 10s   rule GONE
b3  friend-list Spajchar                  expect Spajchar.where=offline   within 10s   rule GONE
b3  friend-list Spapchar                  expect Spapchar.where=offline   within 10s   rule GONE
b3  friend-list Spavchar                  expect Spavchar.where=offline   within 10s   rule GONE
b4  friend-list Spakchar                  expect Spakchar.where=offline   within 10s   rule GONE
b4  friend-list Spaqchar                  expect Spaqchar.where=offline   within 10s   rule GONE
b4  friend-list Spawchar                  expect Spawchar.where=offline   within 10s   rule GONE
b5  friend-list Spalchar                  expect Spalchar.where=offline   within 10s   rule GONE
b5  friend-list Sparchar                  expect Sparchar.where=offline   within 10s   rule GONE
b5  friend-list Spaxchar                  expect Spaxchar.where=offline   within 10s   rule GONE
b0  users global Spagchar Spaxchar        expect count=6 has.Spagchar=no has.Spaxchar=no   within 10s   rule GONE
b1  users global Spagchar Spaxchar        expect count=6 has.Spagchar=no has.Spaxchar=no   within 10s   rule GONE
