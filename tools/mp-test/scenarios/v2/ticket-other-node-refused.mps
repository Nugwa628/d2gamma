# A join ticket names its game server (plan N3): another one refuses it,
# and it still seats on its own.
name    ticket-other-node-refused
about   A ticket made for n1 is shown to n2 first; n2 refuses it; n1 then takes it.
rule    TICKET  a join ticket is good only on the game server it names (plan N3)
setup   split:2
needs   node-games

b0  connect
b0  register vic vic-pass-1               expect ok
b0  char-create Vicpal 3                  expect ok
b0  char-select Vicpal                    expect ok
b0  create-game elsewhere enter=no        expect ok node n1 entered=no
b0  ticket n2                             expect refused=TicketInvalid   rule TICKET
b0  ticket n1                             expect ok                      rule TICKET
