# The Guild room (lobby contract 3.1 and 4): its members only, and it
# follows the membership at once.
name   guild-chat
about  Only b0's guild hears the Guild room; b2 outside it is refused; a kicked member stops hearing it at once.
rule   MEMBERS  only the guild's members hear its room, with the speaker's <TAG> (contract 3.1)
rule   OUTSIDE  a player without a guild can't speak there: NotInGuild (contract 8)
rule   KICKED   a kicked member leaves the room at once: refused and no more lines (contract 4)
bots   3

all connect
b0  register kia kia-pass-1               expect ok
b1  register len len-pass-1               expect ok
b2  register mo mo-pass-1                 expect ok
b0  char-create Kiachar 1                 expect ok
b1  char-create Lenchar 5                 expect ok
b2  char-create Mochar 6                  expect ok
b0  char-select Kiachar                   expect ok
b1  char-select Lenchar                   expect ok
b2  char-select Mochar                    expect ok
b0  guild-create NUG Nugwa                expect ok                                  rule MEMBERS
b0  guild-invite Lenchar                  expect ok                                  rule MEMBERS
b1  guild-answer accept                   expect ok                                  rule MEMBERS
b0  heard-in guild                        expect what=guild                          within 3s   rule MEMBERS
b0  say guild members-only                expect ok                                  rule MEMBERS
b1  heard-in guild                        expect last=members-only said.from=Kiachar tag=NUG   within 3s   rule MEMBERS
b2  heard-in guild                        expect lines=0                             rule OUTSIDE
b2  say guild let-me-in                   expect refused=NotInGuild                  rule OUTSIDE
b0  guild-kick Lenchar                    expect ok                                  rule KICKED
b1  say guild still-here                  expect refused=NotInGuild                  rule KICKED
b0  say guild after-the-kick              expect ok                                  rule KICKED
b0  heard-in guild                        expect said=after-the-kick                 within 3s   rule KICKED
wait 1s
b1  heard-in guild                        expect last=members-only                   rule KICKED
b2  heard-in guild                        expect lines=0                             rule OUTSIDE
