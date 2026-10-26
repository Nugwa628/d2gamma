# A game server behind a strict NAT still links out to the main server: the
# link is always outbound (plan N1, NAT modes).
name   node-links-behind-nat
about  n1 sits behind a symmetric NAT and n2 behind a blocked one; both link to the main server.
rule   LINK  a game server links to the main server whatever its NAT, since it dials out (plan N1)
setup  split:2
nat    n1=symmetric n2=blocked

node n1    expect running=yes linked=yes nat=symmetric   within 15s
node n2    expect running=yes linked=yes nat=blocked     within 15s
