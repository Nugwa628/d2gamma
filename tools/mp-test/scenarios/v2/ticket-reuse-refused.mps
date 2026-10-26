# A join ticket seats once (plan N3): shown again to the game server it
# was made for, in the game or after leaving it, it is refused.
name    ticket-reuse-refused
about   A join ticket is used, then shown again to the same game server, during the game and after it; the game server refuses it both times.
rule    TICKET  a join ticket seats once (plan N3)
setup   split:2
needs   node-games

b0  connect
b0  register sol sol-pass-1               expect ok
b0  char-create Solpal 3                  expect ok
b0  char-select Solpal                    expect ok
b0  create-game once                      expect ok node n1
b0  ticket n1                             expect refused=TicketInvalid   rule TICKET
b0  leave-game
b0  ticket n1                             expect refused=TicketInvalid   rule TICKET
