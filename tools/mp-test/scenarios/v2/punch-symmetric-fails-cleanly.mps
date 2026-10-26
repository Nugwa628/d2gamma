# Written ahead of plan module N10; pending until it lands.
name    punch-symmetric-fails-cleanly
about   A joiner behind a symmetric NAT can't reach a host behind one; it gets a clear error and stays in the lobby.
rule    PUNCH  a punch that can't work ends in a clear error, never a hang (plan N10)
setup   host
pending N10: not built yet
