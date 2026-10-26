# Only the link between a game server and the main server is cut: the game
# server notices, and links again once the cut is over.
name   cut-link-node-sees-loss
about  n1's link to the main server is cut; n1 sees the loss and n2 stays linked; after the cut n1 links again.
rule   LOSS   a game server whose link is cut sees it lost within its timeout (plan N1)
rule   OTHERS a cut on one game server's link leaves the others linked
rule   AGAIN  a game server links again once its link is back
setup  split:2

node n1    expect linked=yes              within 15s  rule LOSS
node n2    expect linked=yes              within 15s  rule LOSS
cut-link n1 on
node n1    expect running=yes linked=no cut=yes   within 15s  rule LOSS
node n2    expect linked=yes                                  rule OTHERS
cut-link n1 off
node n1    expect running=yes linked=yes links=2 cut=no   within 20s  rule AGAIN
