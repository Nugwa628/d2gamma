# A hardcore character that dies keeps its ladder row, marked dead
# (lobby contract 6); a softcore character that died plays on.
name   ladder-dead-hc
about  A dead hardcore character stays on its ladder, marked dead, at its rank; a softcore death marks nothing.
rule   DEAD   a dead hardcore character stays on the ladder at its rank, marked dead (contract 6)
rule   SPLIT  hardcore and softcore characters rank apart (contract 6)
bots   1
server ladder=on ladder_refresh_s=0 limits=test

b0  connect
b0  register hana hana-pass-1             expect ok
server season-start Hardcore test
b0  char-create Hanahard 4 hardcore ladder expect ok
b0  char-create Hanabold 2 hardcore ladder expect ok
b0  char-create Hanasoft 1 ladder         expect ok
server ladder-seed Hanahard 30 900000
server ladder-seed Hanabold 20 300000
server ladder-seed Hanasoft 10 50000
b0  ladder hc=yes                         expect ok total=2 first=Hanahard Hanahard.dead=no Hanabold.rank=2   rule SPLIT
b0  ladder                                expect ok total=1 first=Hanasoft                    rule SPLIT
server ladder-seed Hanahard 31 950000 dead
server ladder-seed Hanasoft 11 60000 dead
b0  ladder hc=yes                         expect ok total=2 Hanahard.rank=1 Hanahard.dead=yes Hanahard.level=31 Hanabold.dead=no   rule DEAD
b0  ladder-find Hanahard hc=yes           expect ok rank=1 Hanahard.dead=yes                  rule DEAD
b0  ladder                                expect ok total=1 Hanasoft.dead=no Hanasoft.level=11  rule DEAD
