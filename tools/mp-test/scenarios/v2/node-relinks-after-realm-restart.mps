# The main server restarts; its game servers keep running and link again.
name   node-relinks-after-realm-restart
about  The main server is stopped and started again; n1 and n2 keep running and link to it again.
rule   AGAIN  game servers outlive a main-server restart and link to it again (plan N1)
setup  split:2

node n1    expect linked=yes              within 15s
node n2    expect linked=yes              within 15s
restart-realm
node n1    expect running=yes linked=yes links=2 starts=1  within 30s
node n2    expect running=yes linked=yes links=2 starts=1  within 30s
