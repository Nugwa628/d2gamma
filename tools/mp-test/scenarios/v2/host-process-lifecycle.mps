# A hosting process (contracts 11.12) started by a player's client: it links,
# goes to sleep when told, and exits by itself on stop.
name   host-process-lifecycle
about  b0's hosting process starts and links; on sleep it closes its game; on stop it exits by itself.
rule   START  a hosting process started with a start line links to the main server (contracts 11.12)
rule   SLEEP  on sleep a hosting process closes its game and says so
rule   STOP   on stop a hosting process says it closed and exits by itself
setup  host

node b0    expect running=yes linked=yes host=yes    within 15s  rule START
host-sleep b0
node b0    expect closed=sleep                       within 10s  rule SLEEP
host-quit b0
node b0    expect running=no closed=stop             within 10s  rule STOP
