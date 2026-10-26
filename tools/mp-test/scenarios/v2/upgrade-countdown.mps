# The upgrade's countdown (server-ctl upgrade): the main server says it in
# Global and every game server says it in every game, at its marks; at zero
# every game saves and closes, the game servers restart on the new version,
# and once the main server restarts on it too they link again. A test
# countdown's minute is 2 s here (`upgrade VERSION MINUTES MINUTE_MS` on a
# test server's stdin); the game servers stage every version at once
# (`update_command = test`) and the setup starts one that exits for a swap
# again as the staged version, as its supervisor does. A game server of
# another version is refused and gets no players; with its updater it
# catches up with the main server's version and links.
name   upgrade-countdown
about  A 3-minute test countdown to 0.0.0.2 is heard in Global and in both games at 3, 2 and 1 minutes and 30 seconds; at zero both games save, n1 and n2 restart on 0.0.0.2 and link to the main server once it runs 0.0.0.2; a game server of another version is refused, and catches up with its updater.
rule   COUNT    the countdown is heard in Global and in every game, at its marks and not before
rule   SWAP     at zero every game saves; every game server restarts on the new version and links to the main server running it
rule   REFUSE   a game server of another version is refused with its reason and gets no players
rule   CATCH    a game server of another version with its updater fetches the main server's version and links
bots   3
setup  split:2
needs  node-games upgrade

all connect
b0  register uma uma-pass-1                expect ok
b1  register ugo ugo-pass-1                expect ok
b2  register ula ula-pass-1                expect ok
b0  char-create Umapal 3                   expect ok
b1  char-create Ugosorc 1                  expect ok
b2  char-create Ulabar 4                   expect ok
b0  char-select Umapal                     expect ok
b1  char-select Ugosorc                    expect ok
b2  char-select Ulabar                     expect ok
b0  create-game up-one                     expect ok node n1   rule SWAP
# n1's load (a player) reaches the main server with its next heartbeat (2 s):
# the next game goes to the emptier n2.
wait 3s
b1  create-game up-two                     expect ok node n2   rule SWAP
b2  heard-in global                        expect ok
node n1                                    expect linked=yes swaps=0   rule SWAP
node n2                                    expect linked=yes swaps=0   rule SWAP

server upgrade 0.0.0.2 3 2000
b2  heard-in global Restarting for an update in 3 minutes   expect has=yes what=notice   within 2s  rule COUNT
b0  heard-game Restarting for an update in 3 minutes        expect has=yes               within 2s  rule COUNT
b1  heard-game Restarting for an update in 3 minutes        expect has=yes               within 2s  rule COUNT
b2  heard-in global Restarting for an update in 2 minutes   expect has=no                           rule COUNT
b0  heard-game Restarting for an update in 2 minutes        expect has=no                           rule COUNT
b2  heard-in global Restarting for an update in 2 minutes   expect has=yes times=1       within 4s  rule COUNT
b0  heard-game Restarting for an update in 2 minutes        expect has=yes times=1       within 1s  rule COUNT
b1  heard-game Restarting for an update in 2 minutes        expect has=yes times=1       within 1s  rule COUNT
b2  heard-in global Restarting for an update in 1 minute    expect has=no                           rule COUNT
b2  heard-in global Restarting for an update in 1 minute    expect has=yes               within 4s  rule COUNT
b0  heard-game Restarting for an update in 1 minute         expect has=yes               within 1s  rule COUNT
b1  heard-game Restarting for an update in 1 minute         expect has=yes               within 1s  rule COUNT
b2  heard-in global Restarting for an update in 30 seconds  expect has=yes               within 3s  rule COUNT
b0  heard-game Restarting for an update in 30 seconds       expect has=yes               within 1s  rule COUNT
b1  heard-game Restarting for an update in 30 seconds       expect has=yes               within 1s  rule COUNT
b0  heard-game Restarting for an update now                 expect has=yes               within 4s  rule COUNT
b1  heard-game Restarting for an update now                 expect has=yes               within 1s  rule COUNT

# Zero: the games save and close, the game servers exit for the swap and
# start again as 0.0.0.2; the main server, at phase swap, restarts as 0.0.0.2.
# The closed games are gone (a resume of one is refused); the saves are kept.
node n1                                    expect swaps=1 version=0.0.0.2   within 30s  rule SWAP
node n2                                    expect swaps=1 version=0.0.0.2   within 30s  rule SWAP
restart-realm version=0.0.0.2                                               within 60s  rule SWAP
node n1                                    expect running=yes linked=yes swaps=1 version=0.0.0.2   within 40s  rule SWAP
node n2                                    expect running=yes linked=yes swaps=1 version=0.0.0.2   within 40s  rule SWAP
b0  reconnect                              expect refused=UnknownResume                     rule SWAP
b0  connect                                expect ok                                        rule SWAP
b0  login uma uma-pass-1                   expect ok                                        rule SWAP
b0  char-list                              expect Umapal.played=yes                         rule SWAP
b1  reconnect                              expect refused=UnknownResume                     rule SWAP
b1  connect                                expect ok                                        rule SWAP
b1  login ugo ugo-pass-1                   expect ok                                        rule SWAP
b1  char-list                              expect Ugosorc.played=yes                        rule SWAP

# Another version, without its updater: refused, and no game goes to it.
# The main server drops the stopped n2's link once it is silent for its
# timeout (d2-link LOST_AFTER, 10 s); only a refused n2 is left then.
stop-node n2
wait 11s
start-node n2 version=0.0.0.9 update=off   expect error                                     rule REFUSE
node n2                                    expect running=no linked=no refusal=NodeBuild    rule REFUSE
b0  char-select Umapal                     expect ok
b1  char-select Ugosorc                    expect ok
b0  create-game after-one                  expect ok node n1                                rule REFUSE
b1  create-game after-two                  expect ok node n1                                rule REFUSE

# With its updater it is told the main server's version, fetches it and
# starts again on it.
start-node n2 version=0.0.0.9              expect ok                                        rule CATCH
node n2                                    expect running=yes linked=yes version=0.0.0.2 swaps=2   within 20s  rule CATCH
