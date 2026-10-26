# A DM to a player who is away (lobby contract 3.2): kept, counted at their
# next hello, kept in the history, and the read mark clears it.
name   chat-dm-offline
about  b1 logs out, b0 whispers, b1 logs in: unread 1, the line comes as a waited one, the history has it, and the read mark clears it.
rule   STORED   a DM to an offline player waits (SocialDone flag) and is pushed at their next hello (contract 3.2)
rule   UNREAD   the next hello's state counts it; the read mark clears it (contract 3)
bots   2

all connect
b0  register fern fern-pass-1             expect ok
b1  register gus gus-pass-1               expect ok
b0  char-create Fernchar 0                expect ok
b1  char-create Guschar 6                 expect ok
b0  char-select Fernchar                  expect ok
b1  char-select Guschar                   expect ok
b1  logout                                expect ok
wait 1s
b0  whisper Guschar see-you-later         expect ok stored=yes                       rule STORED
b1  connect                               expect ok
b1  login gus gus-pass-1                  expect ok
b1  dm-unread                             expect unread=1 conversations=1 fern.unread=1 pushed.stored=1   within 3s   rule UNREAD
b1  dm-history *fern read=yes             expect count=1 last=see-you-later last.stored=no last.outgoing=no   rule STORED
b1  dm-unread                             expect unread=0 conversations=0 fern.unread=0   within 3s   rule UNREAD
b0  dm-history Guschar                    expect count=1 last.outgoing=yes           rule STORED
