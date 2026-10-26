# The friends and ignore lists' limits (lobby contract 5, 8): friends_max
# and ignore_max per account (FriendLimit); a friend added again is no new
# row; a removed friend frees its place; nobody and oneself are
# UnknownPlayer.
name   friends-limits
about  With friends_max 2 and ignore_max 1, b0 adds two friends, the third is FriendLimit, adding one again is no new row, removing one frees a place; one ignore, the second is FriendLimit; unknown names and oneself are UnknownPlayer.
rule   MAX      the friends list holds at most friends_max, the ignore list ignore_max (FriendLimit, contract 5)
rule   AGAIN    adding a friend again keeps one row (contract 1: safe on a repeat)
rule   UNKNOWN  a name nobody has, and oneself, are UnknownPlayer (contract 8)
bots   4
server limits=test friends_max=2 ignore_max=1

all connect
b0  register ivy ivy-pass-1               expect ok
b1  register jon jon-pass-1               expect ok
b2  register kim kim-pass-1               expect ok
b3  register leo leo-pass-1               expect ok
b0  char-create Ivychar 0                 expect ok
b1  char-create Jonchar 1                 expect ok
b2  char-create Kimchar 2                 expect ok
b3  char-create Leochar 3                 expect ok
b0  char-select Ivychar                   expect ok
b0  friend-add Jonchar                    expect ok                                  rule MAX
b0  friend-add *kim                       expect ok                                  rule MAX
b0  friend-add Leochar                    expect refused=FriendLimit                 rule MAX
b0  friend-add jonchar                    expect ok                                  rule AGAIN
b0  friend-list Jonchar Kimchar Leochar   expect count=2 has.Jonchar=yes has.Kimchar=yes has.Leochar=no   rule AGAIN
b0  friend-remove Jonchar                 expect ok                                  rule MAX
b0  friend-add Leochar                    expect ok                                  rule MAX
b0  friend-list Jonchar Leochar           expect count=2 has.Jonchar=no has.Leochar=yes   rule MAX
b0  friend-ignore Jonchar                 expect ok                                  rule MAX
b0  friend-ignore Kimchar                 expect refused=FriendLimit                 rule MAX
b0  friend-list ignored=yes Jonchar       expect count=1 has.Jonchar=yes Jonchar.mutual=no   rule MAX
b0  friend-add Nobodyhere                 expect refused=UnknownPlayer               rule UNKNOWN
b0  friend-add Ivychar                    expect refused=UnknownPlayer               rule UNKNOWN
b0  friend-ignore *ivy                    expect refused=UnknownPlayer               rule UNKNOWN
