# The admin mute (lobby contract 9): a muted account can't Say or Whisper
# (Muted with the wait) and still reads everything; unmute lifts it.
name   chat-mute
about  The console mutes b0: its Say and Whisper are Muted with the wait, it still hears b1; unmute lets it speak again.
rule   MUTED    a muted account's Say and Whisper are Muted with wait_s (contract 9)
rule   READS    a muted account still reads everything (contract 9)
rule   UNMUTE   unmute lifts the mute (contract 9)
bots   2

all connect
b0  register pia pia-pass-1               expect ok
b1  register quin quin-pass-1             expect ok
b0  char-create Piachar 6                 expect ok
b1  char-create Quinchar 0                expect ok
b0  char-select Piachar                   expect ok
b1  char-select Quinchar                  expect ok
b0  heard-in mod                          expect ok                                  rule READS
server mute pia 10 spam
b0  say mod let-me-speak                  expect refused=Muted wait_s>=590           rule MUTED
b0  whisper Quinchar let-me-speak         expect refused=Muted                       rule MUTED
b1  say mod you-still-read-this           expect ok                                  rule READS
b0  heard-in mod                          expect said=you-still-read-this            within 3s   rule READS
server unmute pia
b0  say mod back-again                    expect ok                                  rule UNMUTE
