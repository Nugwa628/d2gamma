# A DM to a player who is online (lobby contract 3.2): stored, then pushed
# to the sessions of both accounts; by character name or `*account`.
name   chat-dm
about  A whisper to an online player reaches them at once, shows on the sender's side too and is kept in both histories.
rule   LIVE     a DM to an online peer is pushed to the peer and to the sender, not kept waiting (contract 3.2)
rule   HISTORY  the conversation is kept for both accounts, named by character or *account (contract 3.2)
rule   NOBODY   a DM to nobody or to oneself is UnknownPlayer (contract 1, 8)
bots   2

all connect
b0  register dana dana-pass-1             expect ok
b1  register emil emil-pass-1             expect ok
b0  char-create Danachar 4                expect ok
b1  char-create Emilchar 5                expect ok
b0  char-select Danachar                  expect ok
b1  char-select Emilchar                  expect ok
b1  dm-unread                             expect unread=0 pushed=0                   rule LIVE
b0  whisper Emilchar psst-emil            expect ok stored=no                        rule LIVE
b1  dm-unread                             expect unread=1 pushed=1 dana.unread=1     within 3s   rule LIVE
b1  whisper *dana hi-back                 expect ok stored=no                        rule LIVE
b0  dm-history Emilchar                   expect count=2 first=psst-emil last=hi-back last.outgoing=no   rule HISTORY
b1  dm-history *DANA                      expect count=2 last=hi-back last.outgoing=yes last.char=Danachar   rule HISTORY
b0  whisper Danachar to-myself            expect refused=UnknownPlayer               rule NOBODY
b0  whisper Nobodychar into-the-void      expect refused=UnknownPlayer               rule NOBODY
