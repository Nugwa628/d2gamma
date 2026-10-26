# The client that started a hosting process crashes: its stdin closes, and the
# hosting process closes and exits by itself (contracts 11.12).
name   host-process-client-crash
about  b0's game client crashes; its hosting process sees stdin close, says it closed and exits.
rule   GONE  a hosting process whose client is gone closes and exits by itself (contracts 11.12)
setup  host

node b0    expect running=yes linked=yes             within 15s
host-client-crash b0
node b0    expect running=no closed=client-gone      within 10s
