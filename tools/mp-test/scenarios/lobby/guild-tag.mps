# Guild tags (lobby contract 4): the rules, taken in any case, and the
# <TAG> on the members' room lines.
name   guild-tag
about  Bad tags and names are BadGuildName; a taken tag is taken in any case; the tag shows on the member's Global and Mod lines.
rule   RULES    a tag is 2-4 letters or digits, a name 3-24 letters, digits and spaces: else BadGuildName (contract 4, 8)
rule   TAKEN    a tag is unique in any case: GuildTagTaken; found by tag in any case (contract 4)
rule   SHOWN    a member's room lines carry the guild's tag (contract 2, 3)
bots   2

all connect
b0  register nia nia-pass-1               expect ok
b1  register oz oz-pass-1                 expect ok
b0  char-create Niachar 1                 expect ok
b1  char-create Ozchar 2                  expect ok
b0  char-select Niachar                   expect ok
b1  char-select Ozchar                    expect ok
b0  guild-create W Short Tag              expect refused=BadGuildName                rule RULES
b0  guild-create WGXYZ Long Tag           expect refused=BadGuildName                rule RULES
b0  guild-create W! Odd Tag               expect refused=BadGuildName                rule RULES
b0  guild-create NUG Ab                   expect refused=BadGuildName                rule RULES
b0  guild-create NUG Name-With-Dashes     expect refused=BadGuildName                rule RULES
b0  guild-create NUG Nugwa                expect ok                                  rule RULES
b1  guild-create nug Other Guild          expect refused=GuildTagTaken               rule TAKEN
b1  guild-create Nug Other Guild          expect refused=GuildTagTaken               rule TAKEN
b1  guild nug                             expect in=no tag=NUG members=1 leader=Niachar   rule TAKEN
b0  say global tagged-hello               expect ok                                  rule SHOWN
b1  heard-in global                       expect said=tagged-hello said.from=Niachar tag=NUG   within 3s   rule SHOWN
b0  say mod tagged-mod                    expect ok                                  rule SHOWN
b1  heard-in mod                          expect said=tagged-mod tag=NUG              within 3s   rule SHOWN
b1  say global untagged                   expect ok                                  rule SHOWN
b0  heard-in global                       expect said=untagged tag=                  within 3s   rule SHOWN
