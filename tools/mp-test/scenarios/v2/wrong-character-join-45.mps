# A seat is for one character (plan N3): the game server answers a 0x67
# join naming another with error 45, and the character is free again.
name    wrong-character-join-45
about   A client with a ticket for one character joins the game with another of its account; it is refused with error 45, and joins right after with the right one.
rule    SEAT  a game server seats only the character its ticket names; any other join is refused with 45 (plan N3)
setup   split:2
needs   node-games

b0  connect
b0  register uma uma-pass-1               expect ok
b0  char-create Umapal 3                  expect ok
b0  char-create Umasorc 1                 expect ok
b0  char-select Umapal                    expect ok
b0  create-game liar as=Umasorc           expect refused=CharMismatch node n1   rule SEAT
wait 2s
b0  create-game honest                    expect ok node n1                     rule SEAT
