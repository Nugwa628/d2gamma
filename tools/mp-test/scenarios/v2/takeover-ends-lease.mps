# A takeover (a new login to the account) ends the old seat's lease at
# once (plan N2): the old session closes, the game server takes the player
# out, and the character plays again right away. A save the old game
# sends after it is refused (unit test save_after_close_is_refused).
name    takeover-ends-lease
about   A character plays on a game server; its account logs in again elsewhere; the old session closes, its game ends, and the new session plays the character at once.
rule    LEASE  a takeover ends the old seat's lease at once: the old session and its seat end, and the character can play again right away (plan N2)
bots    2
setup   split:2
needs   node-games

all connect
b0  register tia tia-pass-1               expect ok
b0  char-create Tiapal 3                  expect ok
b0  char-select Tiapal                    expect ok
b0  create-game oldgame                   expect ok node n1
b1  login tia tia-pass-1                  expect ok                     rule LEASE
b0  stats                                 expect closed      within 5s  rule LEASE
b1  char-select Tiapal                    expect ok                     rule LEASE
b1  games                                 expect count=0     within 10s  rule LEASE
b1  create-game newgame                   expect ok                     rule LEASE
