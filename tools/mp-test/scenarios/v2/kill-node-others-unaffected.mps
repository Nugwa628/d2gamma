# One game server crashes; the others play on, and it can start again.
name   kill-node-others-unaffected
about  n1 is killed; n2 stays up and linked; n1 starts again and links.
rule   OTHERS  a crashed game server doesn't take the others down (plan N1)
rule   AGAIN   a game server started again links to the main server
setup  split:2

node n1    expect linked=yes              within 15s  rule AGAIN
node n2    expect linked=yes              within 15s  rule AGAIN
kill-node n1
node n1    expect running=no linked=no                        rule OTHERS
node n2    expect running=yes linked=yes                      rule OTHERS
wait 2s
node n2    expect running=yes linked=yes                      rule OTHERS
start-node n1
node n1    expect running=yes linked=yes starts=2  within 15s rule AGAIN
stop-node n2
node n2    expect running=no                                  rule AGAIN
