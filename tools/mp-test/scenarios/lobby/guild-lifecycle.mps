# A guild's life (lobby contract 4): created, invites answered or run out,
# ranks, the leader's rules, and the last member leaving disbands it.
name   guild-lifecycle
about  b0 creates <NUG> and invites b1, b2 and b3; b1 and b3 accept, b2's invite runs out; b0 promotes b1, who can't kick b0 but kicks b3; b0 can't leave until it hands over; the last member leaving disbands and frees the tag.
rule   CREATE   a guild is created with its creator as leader (contract 4)
rule   INVITE   officers and the leader invite by character or *account; the invitee is told and may accept (contract 4)
rule   EXPIRE   an invite lasts 7 days, then answering it is InviteGone (contract 4, 8)
rule   RANKS    the leader promotes; an officer kicks members only, never the leader (contract 4)
rule   LEADER   the leader can't leave while others remain (GuildRank), but can after handing over (contract 4)
rule   DISBAND  the last member leaving disbands the guild and its tag is free at once (contract 4)
bots   4

all connect
b0  register gia gia-pass-1               expect ok
b1  register hal hal-pass-1               expect ok
b2  register ivo ivo-pass-1               expect ok
b3  register jun jun-pass-1               expect ok
b0  char-create Giachar 1                 expect ok
b1  char-create Halchar 2                 expect ok
b2  char-create Ivochar 3                 expect ok
b3  char-create Junchar 4                 expect ok
b0  char-select Giachar                   expect ok
b1  char-select Halchar                   expect ok
b2  char-select Ivochar                   expect ok
b3  char-select Junchar                   expect ok
b0  guild                                 expect in=no                               rule CREATE
b0  guild-create NUG Nugwa                expect ok                                  rule CREATE
b0  guild                                 expect in=yes tag=NUG members=1 my_rank=leader leader=Giachar   rule CREATE
b0  guild-invite Halchar                  expect ok                                  rule INVITE
b0  guild-invite *ivo                     expect ok                                  rule INVITE
b0  guild-invite Junchar                  expect ok                                  rule INVITE
b1  guild                                 expect in=no invites=1 notice=invited notice.by=Giachar   within 3s   rule INVITE
b1  guild-answer accept                   expect ok                                  rule INVITE
b3  guild-answer accept NUG               expect ok                                  rule INVITE
b0  guild                                 expect members=3 Halchar.rank=member Junchar.rank=member notice=joined   within 3s   rule INVITE
server social-clock +8d
b2  guild                                 expect in=no invites=0                     within 3s   rule EXPIRE
b2  guild-answer accept NUG               expect refused=InviteGone                  rule EXPIRE
b0  guild-promote Halchar                 expect ok                                  rule RANKS
b1  guild                                 expect my_rank=officer notice=promoted     within 3s   rule RANKS
b1  guild-kick Giachar                    expect refused=GuildRank                   rule RANKS
b1  guild-kick Junchar                    expect ok                                  rule RANKS
b3  guild                                 expect in=no notice=kicked notice.by=Halchar   within 3s   rule RANKS
b0  guild-leave                           expect refused=GuildRank                   rule LEADER
b0  guild-leader Halchar                  expect ok                                  rule LEADER
b0  guild-leave                           expect ok                                  rule LEADER
b0  guild                                 expect in=no notice=left                   within 3s   rule LEADER
b1  guild                                 expect my_rank=leader members=1 leader=Halchar   within 3s   rule LEADER
b1  guild-leave                           expect ok                                  rule DISBAND
b1  guild                                 expect in=no notice=disbanded              within 3s   rule DISBAND
b2  guild NUG                             expect refused=BadGuildName                rule DISBAND
b2  guild-create NUG Second Guild         expect ok                                  rule DISBAND
b2  guild nug                             expect in=yes tag=NUG members=1             rule DISBAND
