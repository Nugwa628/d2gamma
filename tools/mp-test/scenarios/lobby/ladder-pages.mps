# Ladder pages (lobby contract 6): 60 seeded characters in a season read
# in pages of 50; a find goes to the name's page; the own row comes
# pinned when it is off the page; class, hardcore and classic views are
# apart.
name   ladder-pages
about  60 seeded characters: pages of 50, find by whole name in any case, the own rank pinned off the page, a class filter, and hardcore and classic ladders apart.
rule   PAGES  a page holds at most 50 rows in rank order; the next starts where it ends (contract 6)
rule   FIND   a find names the whole name in any case and answers the page with it; anything else is UnknownPlayer (contract 6)
rule   MINE   the own character's rank comes with every page, its row pinned after the page when it is off it (contract 6)
rule   VIEWS  class, hardcore and classic views rank only their characters (contract 6)
bots   2
server ladder=on ladder_refresh_s=0 limits=test max_characters=32

all connect
b0  register pagea pagea-pass-1           expect ok
b1  register pageb pageb-pass-1           expect ok
server season-start Pages test
b0  char-create Rungaa     0 ladder   expect ok
b1  char-create Rungab     1 ladder   expect ok
b0  char-create Rungac     2 ladder   expect ok
b1  char-create Rungad     3 ladder   expect ok
b0  char-create Rungae     4 ladder   expect ok
b1  char-create Rungaf     5 ladder   expect ok
b0  char-create Rungag     6 ladder   expect ok
b1  char-create Rungah     0 ladder   expect ok
b0  char-create Rungai     1 ladder   expect ok
b1  char-create Rungaj     2 ladder   expect ok
b0  char-create Rungak     3 ladder   expect ok
b1  char-create Rungal     4 ladder   expect ok
b0  char-create Rungam     5 ladder   expect ok
b1  char-create Rungan     6 ladder   expect ok
b0  char-create Rungao     0 ladder   expect ok
b1  char-create Rungap     1 ladder   expect ok
b0  char-create Rungaq     2 ladder   expect ok
b1  char-create Rungar     3 ladder   expect ok
b0  char-create Rungas     4 ladder   expect ok
b1  char-create Rungat     5 ladder   expect ok
b0  char-create Rungau     6 ladder   expect ok
b1  char-create Rungav     0 ladder   expect ok
b0  char-create Rungaw     1 ladder   expect ok
b1  char-create Rungax     2 ladder   expect ok
b0  char-create Rungay     3 ladder   expect ok
b1  char-create Rungaz     4 ladder   expect ok
b0  char-create Rungba     5 ladder   expect ok
b1  char-create Rungbb     6 ladder   expect ok
b0  char-create Rungbc     0 ladder   expect ok
b1  char-create Rungbd     1 ladder   expect ok
b0  char-create Rungbe     2 ladder   expect ok
b1  char-create Rungbf     3 ladder   expect ok
b0  char-create Rungbg     4 ladder   expect ok
b1  char-create Rungbh     5 ladder   expect ok
b0  char-create Rungbi     6 ladder   expect ok
b1  char-create Rungbj     0 ladder   expect ok
b0  char-create Rungbk     1 ladder   expect ok
b1  char-create Rungbl     2 ladder   expect ok
b0  char-create Rungbm     3 ladder   expect ok
b1  char-create Rungbn     4 ladder   expect ok
b0  char-create Rungbo     5 ladder   expect ok
b1  char-create Rungbp     6 ladder   expect ok
b0  char-create Rungbq     0 ladder   expect ok
b1  char-create Rungbr     1 ladder   expect ok
b0  char-create Rungbs     2 ladder   expect ok
b1  char-create Rungbt     3 ladder   expect ok
b0  char-create Rungbu     4 ladder   expect ok
b1  char-create Rungbv     5 ladder   expect ok
b0  char-create Rungbw     6 ladder   expect ok
b1  char-create Rungbx     0 ladder   expect ok
b0  char-create Rungby     1 ladder   expect ok
b1  char-create Rungbz     2 ladder   expect ok
b0  char-create Rungca     3 ladder   expect ok
b1  char-create Rungcb     4 ladder   expect ok
b0  char-create Rungcc     5 ladder   expect ok
b1  char-create Rungcd     6 ladder   expect ok
b0  char-create Rungce     0 ladder   expect ok
b1  char-create Rungcf     1 ladder   expect ok
b0  char-create Rungcg     2 ladder   expect ok
b1  char-create Rungch     3 ladder   expect ok
server ladder-seed Rungaa 70 600000
server ladder-seed Rungab 69 590000
server ladder-seed Rungac 68 580000
server ladder-seed Rungad 67 570000
server ladder-seed Rungae 66 560000
server ladder-seed Rungaf 65 550000
server ladder-seed Rungag 64 540000
server ladder-seed Rungah 63 530000
server ladder-seed Rungai 62 520000
server ladder-seed Rungaj 61 510000
server ladder-seed Rungak 60 500000
server ladder-seed Rungal 59 490000
server ladder-seed Rungam 58 480000
server ladder-seed Rungan 57 470000
server ladder-seed Rungao 56 460000
server ladder-seed Rungap 55 450000
server ladder-seed Rungaq 54 440000
server ladder-seed Rungar 53 430000
server ladder-seed Rungas 52 420000
server ladder-seed Rungat 51 410000
server ladder-seed Rungau 50 400000
server ladder-seed Rungav 49 390000
server ladder-seed Rungaw 48 380000
server ladder-seed Rungax 47 370000
server ladder-seed Rungay 46 360000
server ladder-seed Rungaz 45 350000
server ladder-seed Rungba 44 340000
server ladder-seed Rungbb 43 330000
server ladder-seed Rungbc 42 320000
server ladder-seed Rungbd 41 310000
server ladder-seed Rungbe 40 300000
server ladder-seed Rungbf 39 290000
server ladder-seed Rungbg 38 280000
server ladder-seed Rungbh 37 270000
server ladder-seed Rungbi 36 260000
server ladder-seed Rungbj 35 250000
server ladder-seed Rungbk 34 240000
server ladder-seed Rungbl 33 230000
server ladder-seed Rungbm 32 220000
server ladder-seed Rungbn 31 210000
server ladder-seed Rungbo 30 200000
server ladder-seed Rungbp 29 190000
server ladder-seed Rungbq 28 180000
server ladder-seed Rungbr 27 170000
server ladder-seed Rungbs 26 160000
server ladder-seed Rungbt 25 150000
server ladder-seed Rungbu 24 140000
server ladder-seed Rungbv 23 130000
server ladder-seed Rungbw 22 120000
server ladder-seed Rungbx 21 110000
server ladder-seed Rungby 20 100000
server ladder-seed Rungbz 19 90000
server ladder-seed Rungca 18 80000
server ladder-seed Rungcb 17 70000
server ladder-seed Rungcc 16 60000
server ladder-seed Rungcd 15 50000
server ladder-seed Rungce 14 40000
server ladder-seed Rungcf 13 30000
server ladder-seed Rungcg 12 20000
server ladder-seed Rungch 11 10000
b0  char-create Rungxhard 3 hardcore ladder expect ok
b1  char-create Rungxold 3 classic ladder expect ok
server ladder-seed Rungxhard 5 100
server ladder-seed Rungxold 5 100
b0  char-select Rungce                    expect ok
b0  ladder                                expect ok total=60 first_rank=1 rows=50 first=Rungaa first.level=70 first.exp=600000 Rungbx.rank=50 my_rank=57 pinned=yes Rungce.rank=57 Rungce.me=yes   rule PAGES
b0  ladder from=51                        expect ok total=60 first_rank=51 rows=10 first=Rungby Rungch.rank=60 pinned=no   rule PAGES
b0  ladder from=61                        expect ok total=60 rows=0 my_rank=57 pinned=yes     rule PAGES
b0  ladder rows=10 from=11                expect ok first_rank=11 rows=10 first=Rungak    rule PAGES
b0  ladder me=yes                         expect ok first_rank=51 my_rank=57 pinned=no Rungce.me=yes   rule MINE
b1  ladder                                expect ok my_rank=none pinned=no Rungaa.mine=no Rungab.mine=yes Rungab.me=no   rule MINE
b1  ladder-find Rungce                expect ok rank=57 first_rank=51                     rule FIND
b1  ladder-find RUNGAD         expect ok rank=4 first_rank=1                       rule FIND
b1  ladder-find Rung                      expect refused=UnknownPlayer                        rule FIND
b1  ladder-find Rungadx                expect refused=UnknownPlayer                        rule FIND
b1  ladder class=6                        expect ok total=8 first=Rungag Rungag.rank=1 Rungcd.rank=8   rule VIEWS
b1  ladder-find Rungcd class=6 expect ok rank=8                       rule VIEWS
b1  ladder-find Rungaa class=6         expect refused=UnknownPlayer                        rule VIEWS
b1  ladder hc=yes                         expect ok total=1 first=Rungxhard                   rule VIEWS
b1  ladder classic=yes                    expect ok total=1 first=Rungxold                    rule VIEWS
b1  ladder hc=yes classic=yes             expect ok total=0                                   rule VIEWS
