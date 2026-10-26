# The realm chat's rooms (lobby contract 3.1): Global and Mod reach every
# session that said hello, a later hello gets the backlog, and the rooms a
# session may not use are refused.
name   chat-rooms
about  Global and Mod lines reach every session in the room; a later hello gets the backlog; Guild without a guild and the Game room are refused.
rule   GLOBAL   a Global line reaches every session in the room with the speaker's character name (contract 3.1)
rule   MOD      a Mod line reaches every session of the realm's mod game (contract 2, 3.1)
rule   BACKLOG  a hello gets the room's last lines (contract 3.1)
rule   REFUSED  Guild without a guild is NotInGuild; the Game room is never the realm's: RoomUnavailable (contract 2, 8)
bots   3

all connect
b0  register ana ana-pass-1               expect ok
b1  register bob bob-pass-1               expect ok
b2  register cyd cyd-pass-1               expect ok
b0  char-create Anachar 1                 expect ok
b1  char-create Bobchar 2                 expect ok
b2  char-create Cydchar 3                 expect ok
b0  char-select Anachar                   expect ok
b1  char-select Bobchar                   expect ok
b2  char-select Cydchar                   expect ok
b0  heard-in global                       expect ok                                  rule GLOBAL
b1  heard-in global                       expect ok                                  rule GLOBAL
b0  say global hello-world                expect ok                                  rule GLOBAL
b1  heard-in global                       expect said=hello-world said.from=Anachar  within 3s   rule GLOBAL
b0  heard-in global                       expect said=hello-world said.from=Anachar  within 3s   rule GLOBAL
b1  say mod hi-from-bob                   expect ok                                  rule MOD
b0  heard-in mod                          expect said=hi-from-bob said.from=Bobchar  within 3s   rule MOD
b2  heard-in global                       expect said=hello-world backlog>=1         within 3s   rule BACKLOG
b2  heard-in mod                          expect said=hi-from-bob backlog>=1         within 3s   rule BACKLOG
b2  say mod waves emote=yes               expect ok                                  rule MOD
b0  heard-in mod                          expect said=waves what=emote said.from=Cydchar   within 3s   rule MOD
b0  say guild anyone-there                expect refused=NotInGuild                  rule REFUSED
b0  say game not-here                     expect refused=RoomUnavailable             rule REFUSED
