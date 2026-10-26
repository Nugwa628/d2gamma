# A room's user list (lobby contract 3): sorted rows, and a watched list
# kept live by joined, left and changed pushes with the count.
name   users-watch
about  b0 watches the Mod room's user list: b1 and b2 join, b1 leaves; the rows are sorted and the pushes keep the list and its count.
rule   LIST     UserList answers the room's members, sorted by character name (contract 3)
rule   WATCH    a watched list gets RoomPresence for every member who joins, leaves or changes (contract 3)
bots   3

all connect
b0  register rex rex-pass-1               expect ok
b1  register sam sam-pass-1               expect ok
b2  register ted ted-pass-1               expect ok
b0  char-create Zedchar 1                 expect ok
b1  char-create Amychar 2                 expect ok
b2  char-create Mixchar 3                 expect ok
b0  char-select Zedchar                   expect ok
b1  char-select Amychar                   expect ok
b2  char-select Mixchar                   expect ok
b0  users mod Zedchar Amychar watch=yes   expect count=1 has.Zedchar=yes has.Amychar=no   rule LIST
b1  heard-in mod                          expect ok                                  rule WATCH
b2  heard-in mod                          expect ok                                  rule WATCH
b0  users mod Amychar Mixchar             expect count=3 has.Amychar=yes has.Mixchar=yes Amychar.where=lobby joined=2   within 3s   rule WATCH
b2  users mod Amychar Mixchar Zedchar     expect count=3 has.Zedchar=yes             rule LIST
b1  logout                                expect ok                                  rule WATCH
b0  users mod Amychar                     expect count=2 has.Amychar=no left=1       within 3s   rule WATCH
b0  users mod Amychar watch=no            expect count=2                             rule LIST
