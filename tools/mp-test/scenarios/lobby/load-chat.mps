# Many players chatting at once (lobby plan L17, contract 3.1, 3.2 and 9):
# 64 sessions in Global, each saying five lines there and whispering five
# others, at the contract's rate limits. Written by
# tools/mp-test/gen_lobby_load.py: change that, not this file.
name   load-chat
about  64 bots: each says 5 Global lines (one per round, 16 s apart: 2 per 30 s) and whispers 5 bots; every bot hears every line once within 3 s of the round, and every whisper.
rule   HEARD    every Global line reaches every session in the room within 3 s, once (contract 3.1)
rule   WHISPER  every whisper to an online player is pushed to it (contract 3.2)
rule   LIMITS   lines at the contract's rates are never refused (contract 9)
bots   64

all connect
b0  register lcaa lcaa-pass-1             expect ok
b1  register lcab lcab-pass-1             expect ok
b2  register lcac lcac-pass-1             expect ok
b3  register lcad lcad-pass-1             expect ok
b4  register lcae lcae-pass-1             expect ok
b5  register lcaf lcaf-pass-1             expect ok
b6  register lcag lcag-pass-1             expect ok
b7  register lcah lcah-pass-1             expect ok
b8  register lcai lcai-pass-1             expect ok
b9  register lcaj lcaj-pass-1             expect ok
b10 register lcak lcak-pass-1             expect ok
b11 register lcal lcal-pass-1             expect ok
b12 register lcam lcam-pass-1             expect ok
b13 register lcan lcan-pass-1             expect ok
b14 register lcao lcao-pass-1             expect ok
b15 register lcap lcap-pass-1             expect ok
b16 register lcaq lcaq-pass-1             expect ok
b17 register lcar lcar-pass-1             expect ok
b18 register lcas lcas-pass-1             expect ok
b19 register lcat lcat-pass-1             expect ok
b20 register lcau lcau-pass-1             expect ok
b21 register lcav lcav-pass-1             expect ok
b22 register lcaw lcaw-pass-1             expect ok
b23 register lcax lcax-pass-1             expect ok
b24 register lcay lcay-pass-1             expect ok
b25 register lcaz lcaz-pass-1             expect ok
b26 register lcba lcba-pass-1             expect ok
b27 register lcbb lcbb-pass-1             expect ok
b28 register lcbc lcbc-pass-1             expect ok
b29 register lcbd lcbd-pass-1             expect ok
b30 register lcbe lcbe-pass-1             expect ok
b31 register lcbf lcbf-pass-1             expect ok
b32 register lcbg lcbg-pass-1             expect ok
b33 register lcbh lcbh-pass-1             expect ok
b34 register lcbi lcbi-pass-1             expect ok
b35 register lcbj lcbj-pass-1             expect ok
b36 register lcbk lcbk-pass-1             expect ok
b37 register lcbl lcbl-pass-1             expect ok
b38 register lcbm lcbm-pass-1             expect ok
b39 register lcbn lcbn-pass-1             expect ok
b40 register lcbo lcbo-pass-1             expect ok
b41 register lcbp lcbp-pass-1             expect ok
b42 register lcbq lcbq-pass-1             expect ok
b43 register lcbr lcbr-pass-1             expect ok
b44 register lcbs lcbs-pass-1             expect ok
b45 register lcbt lcbt-pass-1             expect ok
b46 register lcbu lcbu-pass-1             expect ok
b47 register lcbv lcbv-pass-1             expect ok
b48 register lcbw lcbw-pass-1             expect ok
b49 register lcbx lcbx-pass-1             expect ok
b50 register lcby lcby-pass-1             expect ok
b51 register lcbz lcbz-pass-1             expect ok
b52 register lcca lcca-pass-1             expect ok
b53 register lccb lccb-pass-1             expect ok
b54 register lccc lccc-pass-1             expect ok
b55 register lccd lccd-pass-1             expect ok
b56 register lcce lcce-pass-1             expect ok
b57 register lccf lccf-pass-1             expect ok
b58 register lccg lccg-pass-1             expect ok
b59 register lcch lcch-pass-1             expect ok
b60 register lcci lcci-pass-1             expect ok
b61 register lccj lccj-pass-1             expect ok
b62 register lcck lcck-pass-1             expect ok
b63 register lccl lccl-pass-1             expect ok
b0  char-create Lcaachar 0                expect ok
b1  char-create Lcabchar 1                expect ok
b2  char-create Lcacchar 2                expect ok
b3  char-create Lcadchar 3                expect ok
b4  char-create Lcaechar 4                expect ok
b5  char-create Lcafchar 5                expect ok
b6  char-create Lcagchar 6                expect ok
b7  char-create Lcahchar 0                expect ok
b8  char-create Lcaichar 1                expect ok
b9  char-create Lcajchar 2                expect ok
b10 char-create Lcakchar 3                expect ok
b11 char-create Lcalchar 4                expect ok
b12 char-create Lcamchar 5                expect ok
b13 char-create Lcanchar 6                expect ok
b14 char-create Lcaochar 0                expect ok
b15 char-create Lcapchar 1                expect ok
b16 char-create Lcaqchar 2                expect ok
b17 char-create Lcarchar 3                expect ok
b18 char-create Lcaschar 4                expect ok
b19 char-create Lcatchar 5                expect ok
b20 char-create Lcauchar 6                expect ok
b21 char-create Lcavchar 0                expect ok
b22 char-create Lcawchar 1                expect ok
b23 char-create Lcaxchar 2                expect ok
b24 char-create Lcaychar 3                expect ok
b25 char-create Lcazchar 4                expect ok
b26 char-create Lcbachar 5                expect ok
b27 char-create Lcbbchar 6                expect ok
b28 char-create Lcbcchar 0                expect ok
b29 char-create Lcbdchar 1                expect ok
b30 char-create Lcbechar 2                expect ok
b31 char-create Lcbfchar 3                expect ok
b32 char-create Lcbgchar 4                expect ok
b33 char-create Lcbhchar 5                expect ok
b34 char-create Lcbichar 6                expect ok
b35 char-create Lcbjchar 0                expect ok
b36 char-create Lcbkchar 1                expect ok
b37 char-create Lcblchar 2                expect ok
b38 char-create Lcbmchar 3                expect ok
b39 char-create Lcbnchar 4                expect ok
b40 char-create Lcbochar 5                expect ok
b41 char-create Lcbpchar 6                expect ok
b42 char-create Lcbqchar 0                expect ok
b43 char-create Lcbrchar 1                expect ok
b44 char-create Lcbschar 2                expect ok
b45 char-create Lcbtchar 3                expect ok
b46 char-create Lcbuchar 4                expect ok
b47 char-create Lcbvchar 5                expect ok
b48 char-create Lcbwchar 6                expect ok
b49 char-create Lcbxchar 0                expect ok
b50 char-create Lcbychar 1                expect ok
b51 char-create Lcbzchar 2                expect ok
b52 char-create Lccachar 3                expect ok
b53 char-create Lccbchar 4                expect ok
b54 char-create Lcccchar 5                expect ok
b55 char-create Lccdchar 6                expect ok
b56 char-create Lccechar 0                expect ok
b57 char-create Lccfchar 1                expect ok
b58 char-create Lccgchar 2                expect ok
b59 char-create Lcchchar 3                expect ok
b60 char-create Lccichar 4                expect ok
b61 char-create Lccjchar 5                expect ok
b62 char-create Lcckchar 6                expect ok
b63 char-create Lcclchar 0                expect ok
b0  char-select Lcaachar                  expect ok
b1  char-select Lcabchar                  expect ok
b2  char-select Lcacchar                  expect ok
b3  char-select Lcadchar                  expect ok
b4  char-select Lcaechar                  expect ok
b5  char-select Lcafchar                  expect ok
b6  char-select Lcagchar                  expect ok
b7  char-select Lcahchar                  expect ok
b8  char-select Lcaichar                  expect ok
b9  char-select Lcajchar                  expect ok
b10 char-select Lcakchar                  expect ok
b11 char-select Lcalchar                  expect ok
b12 char-select Lcamchar                  expect ok
b13 char-select Lcanchar                  expect ok
b14 char-select Lcaochar                  expect ok
b15 char-select Lcapchar                  expect ok
b16 char-select Lcaqchar                  expect ok
b17 char-select Lcarchar                  expect ok
b18 char-select Lcaschar                  expect ok
b19 char-select Lcatchar                  expect ok
b20 char-select Lcauchar                  expect ok
b21 char-select Lcavchar                  expect ok
b22 char-select Lcawchar                  expect ok
b23 char-select Lcaxchar                  expect ok
b24 char-select Lcaychar                  expect ok
b25 char-select Lcazchar                  expect ok
b26 char-select Lcbachar                  expect ok
b27 char-select Lcbbchar                  expect ok
b28 char-select Lcbcchar                  expect ok
b29 char-select Lcbdchar                  expect ok
b30 char-select Lcbechar                  expect ok
b31 char-select Lcbfchar                  expect ok
b32 char-select Lcbgchar                  expect ok
b33 char-select Lcbhchar                  expect ok
b34 char-select Lcbichar                  expect ok
b35 char-select Lcbjchar                  expect ok
b36 char-select Lcbkchar                  expect ok
b37 char-select Lcblchar                  expect ok
b38 char-select Lcbmchar                  expect ok
b39 char-select Lcbnchar                  expect ok
b40 char-select Lcbochar                  expect ok
b41 char-select Lcbpchar                  expect ok
b42 char-select Lcbqchar                  expect ok
b43 char-select Lcbrchar                  expect ok
b44 char-select Lcbschar                  expect ok
b45 char-select Lcbtchar                  expect ok
b46 char-select Lcbuchar                  expect ok
b47 char-select Lcbvchar                  expect ok
b48 char-select Lcbwchar                  expect ok
b49 char-select Lcbxchar                  expect ok
b50 char-select Lcbychar                  expect ok
b51 char-select Lcbzchar                  expect ok
b52 char-select Lccachar                  expect ok
b53 char-select Lccbchar                  expect ok
b54 char-select Lcccchar                  expect ok
b55 char-select Lccdchar                  expect ok
b56 char-select Lccechar                  expect ok
b57 char-select Lccfchar                  expect ok
b58 char-select Lccgchar                  expect ok
b59 char-select Lcchchar                  expect ok
b60 char-select Lccichar                  expect ok
b61 char-select Lccjchar                  expect ok
b62 char-select Lcckchar                  expect ok
b63 char-select Lcclchar                  expect ok
b0  heard-in global                       expect said.count=0   rule HEARD
b1  heard-in global                       expect said.count=0   rule HEARD
b2  heard-in global                       expect said.count=0   rule HEARD
b3  heard-in global                       expect said.count=0   rule HEARD
b4  heard-in global                       expect said.count=0   rule HEARD
b5  heard-in global                       expect said.count=0   rule HEARD
b6  heard-in global                       expect said.count=0   rule HEARD
b7  heard-in global                       expect said.count=0   rule HEARD
b8  heard-in global                       expect said.count=0   rule HEARD
b9  heard-in global                       expect said.count=0   rule HEARD
b10 heard-in global                       expect said.count=0   rule HEARD
b11 heard-in global                       expect said.count=0   rule HEARD
b12 heard-in global                       expect said.count=0   rule HEARD
b13 heard-in global                       expect said.count=0   rule HEARD
b14 heard-in global                       expect said.count=0   rule HEARD
b15 heard-in global                       expect said.count=0   rule HEARD
b16 heard-in global                       expect said.count=0   rule HEARD
b17 heard-in global                       expect said.count=0   rule HEARD
b18 heard-in global                       expect said.count=0   rule HEARD
b19 heard-in global                       expect said.count=0   rule HEARD
b20 heard-in global                       expect said.count=0   rule HEARD
b21 heard-in global                       expect said.count=0   rule HEARD
b22 heard-in global                       expect said.count=0   rule HEARD
b23 heard-in global                       expect said.count=0   rule HEARD
b24 heard-in global                       expect said.count=0   rule HEARD
b25 heard-in global                       expect said.count=0   rule HEARD
b26 heard-in global                       expect said.count=0   rule HEARD
b27 heard-in global                       expect said.count=0   rule HEARD
b28 heard-in global                       expect said.count=0   rule HEARD
b29 heard-in global                       expect said.count=0   rule HEARD
b30 heard-in global                       expect said.count=0   rule HEARD
b31 heard-in global                       expect said.count=0   rule HEARD
b32 heard-in global                       expect said.count=0   rule HEARD
b33 heard-in global                       expect said.count=0   rule HEARD
b34 heard-in global                       expect said.count=0   rule HEARD
b35 heard-in global                       expect said.count=0   rule HEARD
b36 heard-in global                       expect said.count=0   rule HEARD
b37 heard-in global                       expect said.count=0   rule HEARD
b38 heard-in global                       expect said.count=0   rule HEARD
b39 heard-in global                       expect said.count=0   rule HEARD
b40 heard-in global                       expect said.count=0   rule HEARD
b41 heard-in global                       expect said.count=0   rule HEARD
b42 heard-in global                       expect said.count=0   rule HEARD
b43 heard-in global                       expect said.count=0   rule HEARD
b44 heard-in global                       expect said.count=0   rule HEARD
b45 heard-in global                       expect said.count=0   rule HEARD
b46 heard-in global                       expect said.count=0   rule HEARD
b47 heard-in global                       expect said.count=0   rule HEARD
b48 heard-in global                       expect said.count=0   rule HEARD
b49 heard-in global                       expect said.count=0   rule HEARD
b50 heard-in global                       expect said.count=0   rule HEARD
b51 heard-in global                       expect said.count=0   rule HEARD
b52 heard-in global                       expect said.count=0   rule HEARD
b53 heard-in global                       expect said.count=0   rule HEARD
b54 heard-in global                       expect said.count=0   rule HEARD
b55 heard-in global                       expect said.count=0   rule HEARD
b56 heard-in global                       expect said.count=0   rule HEARD
b57 heard-in global                       expect said.count=0   rule HEARD
b58 heard-in global                       expect said.count=0   rule HEARD
b59 heard-in global                       expect said.count=0   rule HEARD
b60 heard-in global                       expect said.count=0   rule HEARD
b61 heard-in global                       expect said.count=0   rule HEARD
b62 heard-in global                       expect said.count=0   rule HEARD
b63 heard-in global                       expect said.count=0   rule HEARD

# Round 1: a Global line and a whisper from every bot.
b0  say global round-a-from-lcaa          expect ok   rule LIMITS
b1  say global round-a-from-lcab          expect ok   rule LIMITS
b2  say global round-a-from-lcac          expect ok   rule LIMITS
b3  say global round-a-from-lcad          expect ok   rule LIMITS
b4  say global round-a-from-lcae          expect ok   rule LIMITS
b5  say global round-a-from-lcaf          expect ok   rule LIMITS
b6  say global round-a-from-lcag          expect ok   rule LIMITS
b7  say global round-a-from-lcah          expect ok   rule LIMITS
b8  say global round-a-from-lcai          expect ok   rule LIMITS
b9  say global round-a-from-lcaj          expect ok   rule LIMITS
b10 say global round-a-from-lcak          expect ok   rule LIMITS
b11 say global round-a-from-lcal          expect ok   rule LIMITS
b12 say global round-a-from-lcam          expect ok   rule LIMITS
b13 say global round-a-from-lcan          expect ok   rule LIMITS
b14 say global round-a-from-lcao          expect ok   rule LIMITS
b15 say global round-a-from-lcap          expect ok   rule LIMITS
b16 say global round-a-from-lcaq          expect ok   rule LIMITS
b17 say global round-a-from-lcar          expect ok   rule LIMITS
b18 say global round-a-from-lcas          expect ok   rule LIMITS
b19 say global round-a-from-lcat          expect ok   rule LIMITS
b20 say global round-a-from-lcau          expect ok   rule LIMITS
b21 say global round-a-from-lcav          expect ok   rule LIMITS
b22 say global round-a-from-lcaw          expect ok   rule LIMITS
b23 say global round-a-from-lcax          expect ok   rule LIMITS
b24 say global round-a-from-lcay          expect ok   rule LIMITS
b25 say global round-a-from-lcaz          expect ok   rule LIMITS
b26 say global round-a-from-lcba          expect ok   rule LIMITS
b27 say global round-a-from-lcbb          expect ok   rule LIMITS
b28 say global round-a-from-lcbc          expect ok   rule LIMITS
b29 say global round-a-from-lcbd          expect ok   rule LIMITS
b30 say global round-a-from-lcbe          expect ok   rule LIMITS
b31 say global round-a-from-lcbf          expect ok   rule LIMITS
b32 say global round-a-from-lcbg          expect ok   rule LIMITS
b33 say global round-a-from-lcbh          expect ok   rule LIMITS
b34 say global round-a-from-lcbi          expect ok   rule LIMITS
b35 say global round-a-from-lcbj          expect ok   rule LIMITS
b36 say global round-a-from-lcbk          expect ok   rule LIMITS
b37 say global round-a-from-lcbl          expect ok   rule LIMITS
b38 say global round-a-from-lcbm          expect ok   rule LIMITS
b39 say global round-a-from-lcbn          expect ok   rule LIMITS
b40 say global round-a-from-lcbo          expect ok   rule LIMITS
b41 say global round-a-from-lcbp          expect ok   rule LIMITS
b42 say global round-a-from-lcbq          expect ok   rule LIMITS
b43 say global round-a-from-lcbr          expect ok   rule LIMITS
b44 say global round-a-from-lcbs          expect ok   rule LIMITS
b45 say global round-a-from-lcbt          expect ok   rule LIMITS
b46 say global round-a-from-lcbu          expect ok   rule LIMITS
b47 say global round-a-from-lcbv          expect ok   rule LIMITS
b48 say global round-a-from-lcbw          expect ok   rule LIMITS
b49 say global round-a-from-lcbx          expect ok   rule LIMITS
b50 say global round-a-from-lcby          expect ok   rule LIMITS
b51 say global round-a-from-lcbz          expect ok   rule LIMITS
b52 say global round-a-from-lcca          expect ok   rule LIMITS
b53 say global round-a-from-lccb          expect ok   rule LIMITS
b54 say global round-a-from-lccc          expect ok   rule LIMITS
b55 say global round-a-from-lccd          expect ok   rule LIMITS
b56 say global round-a-from-lcce          expect ok   rule LIMITS
b57 say global round-a-from-lccf          expect ok   rule LIMITS
b58 say global round-a-from-lccg          expect ok   rule LIMITS
b59 say global round-a-from-lcch          expect ok   rule LIMITS
b60 say global round-a-from-lcci          expect ok   rule LIMITS
b61 say global round-a-from-lccj          expect ok   rule LIMITS
b62 say global round-a-from-lcck          expect ok   rule LIMITS
b63 say global round-a-from-lccl          expect ok   rule LIMITS
b0  whisper Lcabchar dm-a-from-lcaa       expect ok   rule LIMITS
b1  whisper Lcacchar dm-a-from-lcab       expect ok   rule LIMITS
b2  whisper Lcadchar dm-a-from-lcac       expect ok   rule LIMITS
b3  whisper Lcaechar dm-a-from-lcad       expect ok   rule LIMITS
b4  whisper Lcafchar dm-a-from-lcae       expect ok   rule LIMITS
b5  whisper Lcagchar dm-a-from-lcaf       expect ok   rule LIMITS
b6  whisper Lcahchar dm-a-from-lcag       expect ok   rule LIMITS
b7  whisper Lcaichar dm-a-from-lcah       expect ok   rule LIMITS
b8  whisper Lcajchar dm-a-from-lcai       expect ok   rule LIMITS
b9  whisper Lcakchar dm-a-from-lcaj       expect ok   rule LIMITS
b10 whisper Lcalchar dm-a-from-lcak       expect ok   rule LIMITS
b11 whisper Lcamchar dm-a-from-lcal       expect ok   rule LIMITS
b12 whisper Lcanchar dm-a-from-lcam       expect ok   rule LIMITS
b13 whisper Lcaochar dm-a-from-lcan       expect ok   rule LIMITS
b14 whisper Lcapchar dm-a-from-lcao       expect ok   rule LIMITS
b15 whisper Lcaqchar dm-a-from-lcap       expect ok   rule LIMITS
b16 whisper Lcarchar dm-a-from-lcaq       expect ok   rule LIMITS
b17 whisper Lcaschar dm-a-from-lcar       expect ok   rule LIMITS
b18 whisper Lcatchar dm-a-from-lcas       expect ok   rule LIMITS
b19 whisper Lcauchar dm-a-from-lcat       expect ok   rule LIMITS
b20 whisper Lcavchar dm-a-from-lcau       expect ok   rule LIMITS
b21 whisper Lcawchar dm-a-from-lcav       expect ok   rule LIMITS
b22 whisper Lcaxchar dm-a-from-lcaw       expect ok   rule LIMITS
b23 whisper Lcaychar dm-a-from-lcax       expect ok   rule LIMITS
b24 whisper Lcazchar dm-a-from-lcay       expect ok   rule LIMITS
b25 whisper Lcbachar dm-a-from-lcaz       expect ok   rule LIMITS
b26 whisper Lcbbchar dm-a-from-lcba       expect ok   rule LIMITS
b27 whisper Lcbcchar dm-a-from-lcbb       expect ok   rule LIMITS
b28 whisper Lcbdchar dm-a-from-lcbc       expect ok   rule LIMITS
b29 whisper Lcbechar dm-a-from-lcbd       expect ok   rule LIMITS
b30 whisper Lcbfchar dm-a-from-lcbe       expect ok   rule LIMITS
b31 whisper Lcbgchar dm-a-from-lcbf       expect ok   rule LIMITS
b32 whisper Lcbhchar dm-a-from-lcbg       expect ok   rule LIMITS
b33 whisper Lcbichar dm-a-from-lcbh       expect ok   rule LIMITS
b34 whisper Lcbjchar dm-a-from-lcbi       expect ok   rule LIMITS
b35 whisper Lcbkchar dm-a-from-lcbj       expect ok   rule LIMITS
b36 whisper Lcblchar dm-a-from-lcbk       expect ok   rule LIMITS
b37 whisper Lcbmchar dm-a-from-lcbl       expect ok   rule LIMITS
b38 whisper Lcbnchar dm-a-from-lcbm       expect ok   rule LIMITS
b39 whisper Lcbochar dm-a-from-lcbn       expect ok   rule LIMITS
b40 whisper Lcbpchar dm-a-from-lcbo       expect ok   rule LIMITS
b41 whisper Lcbqchar dm-a-from-lcbp       expect ok   rule LIMITS
b42 whisper Lcbrchar dm-a-from-lcbq       expect ok   rule LIMITS
b43 whisper Lcbschar dm-a-from-lcbr       expect ok   rule LIMITS
b44 whisper Lcbtchar dm-a-from-lcbs       expect ok   rule LIMITS
b45 whisper Lcbuchar dm-a-from-lcbt       expect ok   rule LIMITS
b46 whisper Lcbvchar dm-a-from-lcbu       expect ok   rule LIMITS
b47 whisper Lcbwchar dm-a-from-lcbv       expect ok   rule LIMITS
b48 whisper Lcbxchar dm-a-from-lcbw       expect ok   rule LIMITS
b49 whisper Lcbychar dm-a-from-lcbx       expect ok   rule LIMITS
b50 whisper Lcbzchar dm-a-from-lcby       expect ok   rule LIMITS
b51 whisper Lccachar dm-a-from-lcbz       expect ok   rule LIMITS
b52 whisper Lccbchar dm-a-from-lcca       expect ok   rule LIMITS
b53 whisper Lcccchar dm-a-from-lccb       expect ok   rule LIMITS
b54 whisper Lccdchar dm-a-from-lccc       expect ok   rule LIMITS
b55 whisper Lccechar dm-a-from-lccd       expect ok   rule LIMITS
b56 whisper Lccfchar dm-a-from-lcce       expect ok   rule LIMITS
b57 whisper Lccgchar dm-a-from-lccf       expect ok   rule LIMITS
b58 whisper Lcchchar dm-a-from-lccg       expect ok   rule LIMITS
b59 whisper Lccichar dm-a-from-lcch       expect ok   rule LIMITS
b60 whisper Lccjchar dm-a-from-lcci       expect ok   rule LIMITS
b61 whisper Lcckchar dm-a-from-lccj       expect ok   rule LIMITS
b62 whisper Lcclchar dm-a-from-lcck       expect ok   rule LIMITS
b63 whisper Lcaachar dm-a-from-lccl       expect ok   rule LIMITS
b0  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b1  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b2  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b3  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b4  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b5  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b6  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b7  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b8  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b9  heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b10 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b11 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b12 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b13 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b14 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b15 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b16 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b17 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b18 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b19 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b20 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b21 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b22 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b23 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b24 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b25 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b26 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b27 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b28 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b29 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b30 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b31 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b32 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b33 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b34 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b35 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b36 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b37 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b38 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b39 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b40 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b41 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b42 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b43 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b44 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b45 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b46 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b47 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b48 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b49 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b50 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b51 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b52 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b53 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b54 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b55 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b56 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b57 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b58 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b59 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b60 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b61 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b62 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b63 heard-in global                       expect said.count=64 said.distinct=64   within 3s   rule HEARD
b0  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b1  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b2  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b3  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b4  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b5  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b6  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b7  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b8  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b9  dm-unread                             expect pushed=1   within 3s   rule WHISPER
b10 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b11 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b12 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b13 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b14 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b15 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b16 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b17 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b18 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b19 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b20 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b21 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b22 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b23 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b24 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b25 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b26 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b27 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b28 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b29 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b30 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b31 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b32 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b33 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b34 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b35 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b36 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b37 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b38 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b39 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b40 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b41 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b42 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b43 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b44 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b45 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b46 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b47 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b48 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b49 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b50 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b51 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b52 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b53 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b54 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b55 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b56 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b57 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b58 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b59 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b60 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b61 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b62 dm-unread                             expect pushed=1   within 3s   rule WHISPER
b63 dm-unread                             expect pushed=1   within 3s   rule WHISPER

# Round 2: a Global line and a whisper from every bot.
wait 16s
b0  say global round-b-from-lcaa          expect ok   rule LIMITS
b1  say global round-b-from-lcab          expect ok   rule LIMITS
b2  say global round-b-from-lcac          expect ok   rule LIMITS
b3  say global round-b-from-lcad          expect ok   rule LIMITS
b4  say global round-b-from-lcae          expect ok   rule LIMITS
b5  say global round-b-from-lcaf          expect ok   rule LIMITS
b6  say global round-b-from-lcag          expect ok   rule LIMITS
b7  say global round-b-from-lcah          expect ok   rule LIMITS
b8  say global round-b-from-lcai          expect ok   rule LIMITS
b9  say global round-b-from-lcaj          expect ok   rule LIMITS
b10 say global round-b-from-lcak          expect ok   rule LIMITS
b11 say global round-b-from-lcal          expect ok   rule LIMITS
b12 say global round-b-from-lcam          expect ok   rule LIMITS
b13 say global round-b-from-lcan          expect ok   rule LIMITS
b14 say global round-b-from-lcao          expect ok   rule LIMITS
b15 say global round-b-from-lcap          expect ok   rule LIMITS
b16 say global round-b-from-lcaq          expect ok   rule LIMITS
b17 say global round-b-from-lcar          expect ok   rule LIMITS
b18 say global round-b-from-lcas          expect ok   rule LIMITS
b19 say global round-b-from-lcat          expect ok   rule LIMITS
b20 say global round-b-from-lcau          expect ok   rule LIMITS
b21 say global round-b-from-lcav          expect ok   rule LIMITS
b22 say global round-b-from-lcaw          expect ok   rule LIMITS
b23 say global round-b-from-lcax          expect ok   rule LIMITS
b24 say global round-b-from-lcay          expect ok   rule LIMITS
b25 say global round-b-from-lcaz          expect ok   rule LIMITS
b26 say global round-b-from-lcba          expect ok   rule LIMITS
b27 say global round-b-from-lcbb          expect ok   rule LIMITS
b28 say global round-b-from-lcbc          expect ok   rule LIMITS
b29 say global round-b-from-lcbd          expect ok   rule LIMITS
b30 say global round-b-from-lcbe          expect ok   rule LIMITS
b31 say global round-b-from-lcbf          expect ok   rule LIMITS
b32 say global round-b-from-lcbg          expect ok   rule LIMITS
b33 say global round-b-from-lcbh          expect ok   rule LIMITS
b34 say global round-b-from-lcbi          expect ok   rule LIMITS
b35 say global round-b-from-lcbj          expect ok   rule LIMITS
b36 say global round-b-from-lcbk          expect ok   rule LIMITS
b37 say global round-b-from-lcbl          expect ok   rule LIMITS
b38 say global round-b-from-lcbm          expect ok   rule LIMITS
b39 say global round-b-from-lcbn          expect ok   rule LIMITS
b40 say global round-b-from-lcbo          expect ok   rule LIMITS
b41 say global round-b-from-lcbp          expect ok   rule LIMITS
b42 say global round-b-from-lcbq          expect ok   rule LIMITS
b43 say global round-b-from-lcbr          expect ok   rule LIMITS
b44 say global round-b-from-lcbs          expect ok   rule LIMITS
b45 say global round-b-from-lcbt          expect ok   rule LIMITS
b46 say global round-b-from-lcbu          expect ok   rule LIMITS
b47 say global round-b-from-lcbv          expect ok   rule LIMITS
b48 say global round-b-from-lcbw          expect ok   rule LIMITS
b49 say global round-b-from-lcbx          expect ok   rule LIMITS
b50 say global round-b-from-lcby          expect ok   rule LIMITS
b51 say global round-b-from-lcbz          expect ok   rule LIMITS
b52 say global round-b-from-lcca          expect ok   rule LIMITS
b53 say global round-b-from-lccb          expect ok   rule LIMITS
b54 say global round-b-from-lccc          expect ok   rule LIMITS
b55 say global round-b-from-lccd          expect ok   rule LIMITS
b56 say global round-b-from-lcce          expect ok   rule LIMITS
b57 say global round-b-from-lccf          expect ok   rule LIMITS
b58 say global round-b-from-lccg          expect ok   rule LIMITS
b59 say global round-b-from-lcch          expect ok   rule LIMITS
b60 say global round-b-from-lcci          expect ok   rule LIMITS
b61 say global round-b-from-lccj          expect ok   rule LIMITS
b62 say global round-b-from-lcck          expect ok   rule LIMITS
b63 say global round-b-from-lccl          expect ok   rule LIMITS
b0  whisper Lcacchar dm-b-from-lcaa       expect ok   rule LIMITS
b1  whisper Lcadchar dm-b-from-lcab       expect ok   rule LIMITS
b2  whisper Lcaechar dm-b-from-lcac       expect ok   rule LIMITS
b3  whisper Lcafchar dm-b-from-lcad       expect ok   rule LIMITS
b4  whisper Lcagchar dm-b-from-lcae       expect ok   rule LIMITS
b5  whisper Lcahchar dm-b-from-lcaf       expect ok   rule LIMITS
b6  whisper Lcaichar dm-b-from-lcag       expect ok   rule LIMITS
b7  whisper Lcajchar dm-b-from-lcah       expect ok   rule LIMITS
b8  whisper Lcakchar dm-b-from-lcai       expect ok   rule LIMITS
b9  whisper Lcalchar dm-b-from-lcaj       expect ok   rule LIMITS
b10 whisper Lcamchar dm-b-from-lcak       expect ok   rule LIMITS
b11 whisper Lcanchar dm-b-from-lcal       expect ok   rule LIMITS
b12 whisper Lcaochar dm-b-from-lcam       expect ok   rule LIMITS
b13 whisper Lcapchar dm-b-from-lcan       expect ok   rule LIMITS
b14 whisper Lcaqchar dm-b-from-lcao       expect ok   rule LIMITS
b15 whisper Lcarchar dm-b-from-lcap       expect ok   rule LIMITS
b16 whisper Lcaschar dm-b-from-lcaq       expect ok   rule LIMITS
b17 whisper Lcatchar dm-b-from-lcar       expect ok   rule LIMITS
b18 whisper Lcauchar dm-b-from-lcas       expect ok   rule LIMITS
b19 whisper Lcavchar dm-b-from-lcat       expect ok   rule LIMITS
b20 whisper Lcawchar dm-b-from-lcau       expect ok   rule LIMITS
b21 whisper Lcaxchar dm-b-from-lcav       expect ok   rule LIMITS
b22 whisper Lcaychar dm-b-from-lcaw       expect ok   rule LIMITS
b23 whisper Lcazchar dm-b-from-lcax       expect ok   rule LIMITS
b24 whisper Lcbachar dm-b-from-lcay       expect ok   rule LIMITS
b25 whisper Lcbbchar dm-b-from-lcaz       expect ok   rule LIMITS
b26 whisper Lcbcchar dm-b-from-lcba       expect ok   rule LIMITS
b27 whisper Lcbdchar dm-b-from-lcbb       expect ok   rule LIMITS
b28 whisper Lcbechar dm-b-from-lcbc       expect ok   rule LIMITS
b29 whisper Lcbfchar dm-b-from-lcbd       expect ok   rule LIMITS
b30 whisper Lcbgchar dm-b-from-lcbe       expect ok   rule LIMITS
b31 whisper Lcbhchar dm-b-from-lcbf       expect ok   rule LIMITS
b32 whisper Lcbichar dm-b-from-lcbg       expect ok   rule LIMITS
b33 whisper Lcbjchar dm-b-from-lcbh       expect ok   rule LIMITS
b34 whisper Lcbkchar dm-b-from-lcbi       expect ok   rule LIMITS
b35 whisper Lcblchar dm-b-from-lcbj       expect ok   rule LIMITS
b36 whisper Lcbmchar dm-b-from-lcbk       expect ok   rule LIMITS
b37 whisper Lcbnchar dm-b-from-lcbl       expect ok   rule LIMITS
b38 whisper Lcbochar dm-b-from-lcbm       expect ok   rule LIMITS
b39 whisper Lcbpchar dm-b-from-lcbn       expect ok   rule LIMITS
b40 whisper Lcbqchar dm-b-from-lcbo       expect ok   rule LIMITS
b41 whisper Lcbrchar dm-b-from-lcbp       expect ok   rule LIMITS
b42 whisper Lcbschar dm-b-from-lcbq       expect ok   rule LIMITS
b43 whisper Lcbtchar dm-b-from-lcbr       expect ok   rule LIMITS
b44 whisper Lcbuchar dm-b-from-lcbs       expect ok   rule LIMITS
b45 whisper Lcbvchar dm-b-from-lcbt       expect ok   rule LIMITS
b46 whisper Lcbwchar dm-b-from-lcbu       expect ok   rule LIMITS
b47 whisper Lcbxchar dm-b-from-lcbv       expect ok   rule LIMITS
b48 whisper Lcbychar dm-b-from-lcbw       expect ok   rule LIMITS
b49 whisper Lcbzchar dm-b-from-lcbx       expect ok   rule LIMITS
b50 whisper Lccachar dm-b-from-lcby       expect ok   rule LIMITS
b51 whisper Lccbchar dm-b-from-lcbz       expect ok   rule LIMITS
b52 whisper Lcccchar dm-b-from-lcca       expect ok   rule LIMITS
b53 whisper Lccdchar dm-b-from-lccb       expect ok   rule LIMITS
b54 whisper Lccechar dm-b-from-lccc       expect ok   rule LIMITS
b55 whisper Lccfchar dm-b-from-lccd       expect ok   rule LIMITS
b56 whisper Lccgchar dm-b-from-lcce       expect ok   rule LIMITS
b57 whisper Lcchchar dm-b-from-lccf       expect ok   rule LIMITS
b58 whisper Lccichar dm-b-from-lccg       expect ok   rule LIMITS
b59 whisper Lccjchar dm-b-from-lcch       expect ok   rule LIMITS
b60 whisper Lcckchar dm-b-from-lcci       expect ok   rule LIMITS
b61 whisper Lcclchar dm-b-from-lccj       expect ok   rule LIMITS
b62 whisper Lcaachar dm-b-from-lcck       expect ok   rule LIMITS
b63 whisper Lcabchar dm-b-from-lccl       expect ok   rule LIMITS
b0  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b1  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b2  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b3  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b4  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b5  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b6  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b7  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b8  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b9  heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b10 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b11 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b12 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b13 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b14 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b15 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b16 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b17 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b18 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b19 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b20 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b21 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b22 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b23 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b24 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b25 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b26 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b27 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b28 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b29 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b30 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b31 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b32 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b33 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b34 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b35 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b36 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b37 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b38 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b39 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b40 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b41 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b42 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b43 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b44 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b45 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b46 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b47 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b48 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b49 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b50 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b51 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b52 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b53 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b54 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b55 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b56 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b57 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b58 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b59 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b60 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b61 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b62 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b63 heard-in global                       expect said.count=128 said.distinct=128   within 3s   rule HEARD
b0  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b1  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b2  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b3  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b4  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b5  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b6  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b7  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b8  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b9  dm-unread                             expect pushed=2   within 3s   rule WHISPER
b10 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b11 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b12 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b13 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b14 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b15 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b16 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b17 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b18 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b19 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b20 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b21 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b22 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b23 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b24 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b25 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b26 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b27 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b28 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b29 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b30 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b31 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b32 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b33 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b34 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b35 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b36 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b37 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b38 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b39 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b40 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b41 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b42 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b43 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b44 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b45 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b46 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b47 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b48 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b49 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b50 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b51 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b52 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b53 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b54 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b55 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b56 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b57 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b58 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b59 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b60 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b61 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b62 dm-unread                             expect pushed=2   within 3s   rule WHISPER
b63 dm-unread                             expect pushed=2   within 3s   rule WHISPER

# Round 3: a Global line and a whisper from every bot.
wait 16s
b0  say global round-c-from-lcaa          expect ok   rule LIMITS
b1  say global round-c-from-lcab          expect ok   rule LIMITS
b2  say global round-c-from-lcac          expect ok   rule LIMITS
b3  say global round-c-from-lcad          expect ok   rule LIMITS
b4  say global round-c-from-lcae          expect ok   rule LIMITS
b5  say global round-c-from-lcaf          expect ok   rule LIMITS
b6  say global round-c-from-lcag          expect ok   rule LIMITS
b7  say global round-c-from-lcah          expect ok   rule LIMITS
b8  say global round-c-from-lcai          expect ok   rule LIMITS
b9  say global round-c-from-lcaj          expect ok   rule LIMITS
b10 say global round-c-from-lcak          expect ok   rule LIMITS
b11 say global round-c-from-lcal          expect ok   rule LIMITS
b12 say global round-c-from-lcam          expect ok   rule LIMITS
b13 say global round-c-from-lcan          expect ok   rule LIMITS
b14 say global round-c-from-lcao          expect ok   rule LIMITS
b15 say global round-c-from-lcap          expect ok   rule LIMITS
b16 say global round-c-from-lcaq          expect ok   rule LIMITS
b17 say global round-c-from-lcar          expect ok   rule LIMITS
b18 say global round-c-from-lcas          expect ok   rule LIMITS
b19 say global round-c-from-lcat          expect ok   rule LIMITS
b20 say global round-c-from-lcau          expect ok   rule LIMITS
b21 say global round-c-from-lcav          expect ok   rule LIMITS
b22 say global round-c-from-lcaw          expect ok   rule LIMITS
b23 say global round-c-from-lcax          expect ok   rule LIMITS
b24 say global round-c-from-lcay          expect ok   rule LIMITS
b25 say global round-c-from-lcaz          expect ok   rule LIMITS
b26 say global round-c-from-lcba          expect ok   rule LIMITS
b27 say global round-c-from-lcbb          expect ok   rule LIMITS
b28 say global round-c-from-lcbc          expect ok   rule LIMITS
b29 say global round-c-from-lcbd          expect ok   rule LIMITS
b30 say global round-c-from-lcbe          expect ok   rule LIMITS
b31 say global round-c-from-lcbf          expect ok   rule LIMITS
b32 say global round-c-from-lcbg          expect ok   rule LIMITS
b33 say global round-c-from-lcbh          expect ok   rule LIMITS
b34 say global round-c-from-lcbi          expect ok   rule LIMITS
b35 say global round-c-from-lcbj          expect ok   rule LIMITS
b36 say global round-c-from-lcbk          expect ok   rule LIMITS
b37 say global round-c-from-lcbl          expect ok   rule LIMITS
b38 say global round-c-from-lcbm          expect ok   rule LIMITS
b39 say global round-c-from-lcbn          expect ok   rule LIMITS
b40 say global round-c-from-lcbo          expect ok   rule LIMITS
b41 say global round-c-from-lcbp          expect ok   rule LIMITS
b42 say global round-c-from-lcbq          expect ok   rule LIMITS
b43 say global round-c-from-lcbr          expect ok   rule LIMITS
b44 say global round-c-from-lcbs          expect ok   rule LIMITS
b45 say global round-c-from-lcbt          expect ok   rule LIMITS
b46 say global round-c-from-lcbu          expect ok   rule LIMITS
b47 say global round-c-from-lcbv          expect ok   rule LIMITS
b48 say global round-c-from-lcbw          expect ok   rule LIMITS
b49 say global round-c-from-lcbx          expect ok   rule LIMITS
b50 say global round-c-from-lcby          expect ok   rule LIMITS
b51 say global round-c-from-lcbz          expect ok   rule LIMITS
b52 say global round-c-from-lcca          expect ok   rule LIMITS
b53 say global round-c-from-lccb          expect ok   rule LIMITS
b54 say global round-c-from-lccc          expect ok   rule LIMITS
b55 say global round-c-from-lccd          expect ok   rule LIMITS
b56 say global round-c-from-lcce          expect ok   rule LIMITS
b57 say global round-c-from-lccf          expect ok   rule LIMITS
b58 say global round-c-from-lccg          expect ok   rule LIMITS
b59 say global round-c-from-lcch          expect ok   rule LIMITS
b60 say global round-c-from-lcci          expect ok   rule LIMITS
b61 say global round-c-from-lccj          expect ok   rule LIMITS
b62 say global round-c-from-lcck          expect ok   rule LIMITS
b63 say global round-c-from-lccl          expect ok   rule LIMITS
b0  whisper Lcadchar dm-c-from-lcaa       expect ok   rule LIMITS
b1  whisper Lcaechar dm-c-from-lcab       expect ok   rule LIMITS
b2  whisper Lcafchar dm-c-from-lcac       expect ok   rule LIMITS
b3  whisper Lcagchar dm-c-from-lcad       expect ok   rule LIMITS
b4  whisper Lcahchar dm-c-from-lcae       expect ok   rule LIMITS
b5  whisper Lcaichar dm-c-from-lcaf       expect ok   rule LIMITS
b6  whisper Lcajchar dm-c-from-lcag       expect ok   rule LIMITS
b7  whisper Lcakchar dm-c-from-lcah       expect ok   rule LIMITS
b8  whisper Lcalchar dm-c-from-lcai       expect ok   rule LIMITS
b9  whisper Lcamchar dm-c-from-lcaj       expect ok   rule LIMITS
b10 whisper Lcanchar dm-c-from-lcak       expect ok   rule LIMITS
b11 whisper Lcaochar dm-c-from-lcal       expect ok   rule LIMITS
b12 whisper Lcapchar dm-c-from-lcam       expect ok   rule LIMITS
b13 whisper Lcaqchar dm-c-from-lcan       expect ok   rule LIMITS
b14 whisper Lcarchar dm-c-from-lcao       expect ok   rule LIMITS
b15 whisper Lcaschar dm-c-from-lcap       expect ok   rule LIMITS
b16 whisper Lcatchar dm-c-from-lcaq       expect ok   rule LIMITS
b17 whisper Lcauchar dm-c-from-lcar       expect ok   rule LIMITS
b18 whisper Lcavchar dm-c-from-lcas       expect ok   rule LIMITS
b19 whisper Lcawchar dm-c-from-lcat       expect ok   rule LIMITS
b20 whisper Lcaxchar dm-c-from-lcau       expect ok   rule LIMITS
b21 whisper Lcaychar dm-c-from-lcav       expect ok   rule LIMITS
b22 whisper Lcazchar dm-c-from-lcaw       expect ok   rule LIMITS
b23 whisper Lcbachar dm-c-from-lcax       expect ok   rule LIMITS
b24 whisper Lcbbchar dm-c-from-lcay       expect ok   rule LIMITS
b25 whisper Lcbcchar dm-c-from-lcaz       expect ok   rule LIMITS
b26 whisper Lcbdchar dm-c-from-lcba       expect ok   rule LIMITS
b27 whisper Lcbechar dm-c-from-lcbb       expect ok   rule LIMITS
b28 whisper Lcbfchar dm-c-from-lcbc       expect ok   rule LIMITS
b29 whisper Lcbgchar dm-c-from-lcbd       expect ok   rule LIMITS
b30 whisper Lcbhchar dm-c-from-lcbe       expect ok   rule LIMITS
b31 whisper Lcbichar dm-c-from-lcbf       expect ok   rule LIMITS
b32 whisper Lcbjchar dm-c-from-lcbg       expect ok   rule LIMITS
b33 whisper Lcbkchar dm-c-from-lcbh       expect ok   rule LIMITS
b34 whisper Lcblchar dm-c-from-lcbi       expect ok   rule LIMITS
b35 whisper Lcbmchar dm-c-from-lcbj       expect ok   rule LIMITS
b36 whisper Lcbnchar dm-c-from-lcbk       expect ok   rule LIMITS
b37 whisper Lcbochar dm-c-from-lcbl       expect ok   rule LIMITS
b38 whisper Lcbpchar dm-c-from-lcbm       expect ok   rule LIMITS
b39 whisper Lcbqchar dm-c-from-lcbn       expect ok   rule LIMITS
b40 whisper Lcbrchar dm-c-from-lcbo       expect ok   rule LIMITS
b41 whisper Lcbschar dm-c-from-lcbp       expect ok   rule LIMITS
b42 whisper Lcbtchar dm-c-from-lcbq       expect ok   rule LIMITS
b43 whisper Lcbuchar dm-c-from-lcbr       expect ok   rule LIMITS
b44 whisper Lcbvchar dm-c-from-lcbs       expect ok   rule LIMITS
b45 whisper Lcbwchar dm-c-from-lcbt       expect ok   rule LIMITS
b46 whisper Lcbxchar dm-c-from-lcbu       expect ok   rule LIMITS
b47 whisper Lcbychar dm-c-from-lcbv       expect ok   rule LIMITS
b48 whisper Lcbzchar dm-c-from-lcbw       expect ok   rule LIMITS
b49 whisper Lccachar dm-c-from-lcbx       expect ok   rule LIMITS
b50 whisper Lccbchar dm-c-from-lcby       expect ok   rule LIMITS
b51 whisper Lcccchar dm-c-from-lcbz       expect ok   rule LIMITS
b52 whisper Lccdchar dm-c-from-lcca       expect ok   rule LIMITS
b53 whisper Lccechar dm-c-from-lccb       expect ok   rule LIMITS
b54 whisper Lccfchar dm-c-from-lccc       expect ok   rule LIMITS
b55 whisper Lccgchar dm-c-from-lccd       expect ok   rule LIMITS
b56 whisper Lcchchar dm-c-from-lcce       expect ok   rule LIMITS
b57 whisper Lccichar dm-c-from-lccf       expect ok   rule LIMITS
b58 whisper Lccjchar dm-c-from-lccg       expect ok   rule LIMITS
b59 whisper Lcckchar dm-c-from-lcch       expect ok   rule LIMITS
b60 whisper Lcclchar dm-c-from-lcci       expect ok   rule LIMITS
b61 whisper Lcaachar dm-c-from-lccj       expect ok   rule LIMITS
b62 whisper Lcabchar dm-c-from-lcck       expect ok   rule LIMITS
b63 whisper Lcacchar dm-c-from-lccl       expect ok   rule LIMITS
b0  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b1  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b2  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b3  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b4  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b5  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b6  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b7  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b8  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b9  heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b10 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b11 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b12 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b13 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b14 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b15 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b16 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b17 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b18 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b19 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b20 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b21 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b22 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b23 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b24 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b25 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b26 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b27 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b28 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b29 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b30 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b31 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b32 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b33 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b34 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b35 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b36 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b37 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b38 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b39 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b40 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b41 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b42 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b43 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b44 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b45 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b46 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b47 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b48 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b49 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b50 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b51 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b52 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b53 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b54 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b55 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b56 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b57 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b58 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b59 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b60 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b61 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b62 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b63 heard-in global                       expect said.count=192 said.distinct=192   within 3s   rule HEARD
b0  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b1  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b2  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b3  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b4  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b5  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b6  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b7  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b8  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b9  dm-unread                             expect pushed=3   within 3s   rule WHISPER
b10 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b11 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b12 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b13 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b14 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b15 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b16 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b17 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b18 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b19 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b20 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b21 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b22 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b23 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b24 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b25 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b26 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b27 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b28 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b29 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b30 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b31 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b32 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b33 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b34 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b35 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b36 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b37 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b38 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b39 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b40 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b41 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b42 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b43 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b44 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b45 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b46 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b47 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b48 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b49 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b50 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b51 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b52 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b53 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b54 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b55 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b56 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b57 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b58 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b59 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b60 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b61 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b62 dm-unread                             expect pushed=3   within 3s   rule WHISPER
b63 dm-unread                             expect pushed=3   within 3s   rule WHISPER

# Round 4: a Global line and a whisper from every bot.
wait 16s
b0  say global round-d-from-lcaa          expect ok   rule LIMITS
b1  say global round-d-from-lcab          expect ok   rule LIMITS
b2  say global round-d-from-lcac          expect ok   rule LIMITS
b3  say global round-d-from-lcad          expect ok   rule LIMITS
b4  say global round-d-from-lcae          expect ok   rule LIMITS
b5  say global round-d-from-lcaf          expect ok   rule LIMITS
b6  say global round-d-from-lcag          expect ok   rule LIMITS
b7  say global round-d-from-lcah          expect ok   rule LIMITS
b8  say global round-d-from-lcai          expect ok   rule LIMITS
b9  say global round-d-from-lcaj          expect ok   rule LIMITS
b10 say global round-d-from-lcak          expect ok   rule LIMITS
b11 say global round-d-from-lcal          expect ok   rule LIMITS
b12 say global round-d-from-lcam          expect ok   rule LIMITS
b13 say global round-d-from-lcan          expect ok   rule LIMITS
b14 say global round-d-from-lcao          expect ok   rule LIMITS
b15 say global round-d-from-lcap          expect ok   rule LIMITS
b16 say global round-d-from-lcaq          expect ok   rule LIMITS
b17 say global round-d-from-lcar          expect ok   rule LIMITS
b18 say global round-d-from-lcas          expect ok   rule LIMITS
b19 say global round-d-from-lcat          expect ok   rule LIMITS
b20 say global round-d-from-lcau          expect ok   rule LIMITS
b21 say global round-d-from-lcav          expect ok   rule LIMITS
b22 say global round-d-from-lcaw          expect ok   rule LIMITS
b23 say global round-d-from-lcax          expect ok   rule LIMITS
b24 say global round-d-from-lcay          expect ok   rule LIMITS
b25 say global round-d-from-lcaz          expect ok   rule LIMITS
b26 say global round-d-from-lcba          expect ok   rule LIMITS
b27 say global round-d-from-lcbb          expect ok   rule LIMITS
b28 say global round-d-from-lcbc          expect ok   rule LIMITS
b29 say global round-d-from-lcbd          expect ok   rule LIMITS
b30 say global round-d-from-lcbe          expect ok   rule LIMITS
b31 say global round-d-from-lcbf          expect ok   rule LIMITS
b32 say global round-d-from-lcbg          expect ok   rule LIMITS
b33 say global round-d-from-lcbh          expect ok   rule LIMITS
b34 say global round-d-from-lcbi          expect ok   rule LIMITS
b35 say global round-d-from-lcbj          expect ok   rule LIMITS
b36 say global round-d-from-lcbk          expect ok   rule LIMITS
b37 say global round-d-from-lcbl          expect ok   rule LIMITS
b38 say global round-d-from-lcbm          expect ok   rule LIMITS
b39 say global round-d-from-lcbn          expect ok   rule LIMITS
b40 say global round-d-from-lcbo          expect ok   rule LIMITS
b41 say global round-d-from-lcbp          expect ok   rule LIMITS
b42 say global round-d-from-lcbq          expect ok   rule LIMITS
b43 say global round-d-from-lcbr          expect ok   rule LIMITS
b44 say global round-d-from-lcbs          expect ok   rule LIMITS
b45 say global round-d-from-lcbt          expect ok   rule LIMITS
b46 say global round-d-from-lcbu          expect ok   rule LIMITS
b47 say global round-d-from-lcbv          expect ok   rule LIMITS
b48 say global round-d-from-lcbw          expect ok   rule LIMITS
b49 say global round-d-from-lcbx          expect ok   rule LIMITS
b50 say global round-d-from-lcby          expect ok   rule LIMITS
b51 say global round-d-from-lcbz          expect ok   rule LIMITS
b52 say global round-d-from-lcca          expect ok   rule LIMITS
b53 say global round-d-from-lccb          expect ok   rule LIMITS
b54 say global round-d-from-lccc          expect ok   rule LIMITS
b55 say global round-d-from-lccd          expect ok   rule LIMITS
b56 say global round-d-from-lcce          expect ok   rule LIMITS
b57 say global round-d-from-lccf          expect ok   rule LIMITS
b58 say global round-d-from-lccg          expect ok   rule LIMITS
b59 say global round-d-from-lcch          expect ok   rule LIMITS
b60 say global round-d-from-lcci          expect ok   rule LIMITS
b61 say global round-d-from-lccj          expect ok   rule LIMITS
b62 say global round-d-from-lcck          expect ok   rule LIMITS
b63 say global round-d-from-lccl          expect ok   rule LIMITS
b0  whisper Lcaechar dm-d-from-lcaa       expect ok   rule LIMITS
b1  whisper Lcafchar dm-d-from-lcab       expect ok   rule LIMITS
b2  whisper Lcagchar dm-d-from-lcac       expect ok   rule LIMITS
b3  whisper Lcahchar dm-d-from-lcad       expect ok   rule LIMITS
b4  whisper Lcaichar dm-d-from-lcae       expect ok   rule LIMITS
b5  whisper Lcajchar dm-d-from-lcaf       expect ok   rule LIMITS
b6  whisper Lcakchar dm-d-from-lcag       expect ok   rule LIMITS
b7  whisper Lcalchar dm-d-from-lcah       expect ok   rule LIMITS
b8  whisper Lcamchar dm-d-from-lcai       expect ok   rule LIMITS
b9  whisper Lcanchar dm-d-from-lcaj       expect ok   rule LIMITS
b10 whisper Lcaochar dm-d-from-lcak       expect ok   rule LIMITS
b11 whisper Lcapchar dm-d-from-lcal       expect ok   rule LIMITS
b12 whisper Lcaqchar dm-d-from-lcam       expect ok   rule LIMITS
b13 whisper Lcarchar dm-d-from-lcan       expect ok   rule LIMITS
b14 whisper Lcaschar dm-d-from-lcao       expect ok   rule LIMITS
b15 whisper Lcatchar dm-d-from-lcap       expect ok   rule LIMITS
b16 whisper Lcauchar dm-d-from-lcaq       expect ok   rule LIMITS
b17 whisper Lcavchar dm-d-from-lcar       expect ok   rule LIMITS
b18 whisper Lcawchar dm-d-from-lcas       expect ok   rule LIMITS
b19 whisper Lcaxchar dm-d-from-lcat       expect ok   rule LIMITS
b20 whisper Lcaychar dm-d-from-lcau       expect ok   rule LIMITS
b21 whisper Lcazchar dm-d-from-lcav       expect ok   rule LIMITS
b22 whisper Lcbachar dm-d-from-lcaw       expect ok   rule LIMITS
b23 whisper Lcbbchar dm-d-from-lcax       expect ok   rule LIMITS
b24 whisper Lcbcchar dm-d-from-lcay       expect ok   rule LIMITS
b25 whisper Lcbdchar dm-d-from-lcaz       expect ok   rule LIMITS
b26 whisper Lcbechar dm-d-from-lcba       expect ok   rule LIMITS
b27 whisper Lcbfchar dm-d-from-lcbb       expect ok   rule LIMITS
b28 whisper Lcbgchar dm-d-from-lcbc       expect ok   rule LIMITS
b29 whisper Lcbhchar dm-d-from-lcbd       expect ok   rule LIMITS
b30 whisper Lcbichar dm-d-from-lcbe       expect ok   rule LIMITS
b31 whisper Lcbjchar dm-d-from-lcbf       expect ok   rule LIMITS
b32 whisper Lcbkchar dm-d-from-lcbg       expect ok   rule LIMITS
b33 whisper Lcblchar dm-d-from-lcbh       expect ok   rule LIMITS
b34 whisper Lcbmchar dm-d-from-lcbi       expect ok   rule LIMITS
b35 whisper Lcbnchar dm-d-from-lcbj       expect ok   rule LIMITS
b36 whisper Lcbochar dm-d-from-lcbk       expect ok   rule LIMITS
b37 whisper Lcbpchar dm-d-from-lcbl       expect ok   rule LIMITS
b38 whisper Lcbqchar dm-d-from-lcbm       expect ok   rule LIMITS
b39 whisper Lcbrchar dm-d-from-lcbn       expect ok   rule LIMITS
b40 whisper Lcbschar dm-d-from-lcbo       expect ok   rule LIMITS
b41 whisper Lcbtchar dm-d-from-lcbp       expect ok   rule LIMITS
b42 whisper Lcbuchar dm-d-from-lcbq       expect ok   rule LIMITS
b43 whisper Lcbvchar dm-d-from-lcbr       expect ok   rule LIMITS
b44 whisper Lcbwchar dm-d-from-lcbs       expect ok   rule LIMITS
b45 whisper Lcbxchar dm-d-from-lcbt       expect ok   rule LIMITS
b46 whisper Lcbychar dm-d-from-lcbu       expect ok   rule LIMITS
b47 whisper Lcbzchar dm-d-from-lcbv       expect ok   rule LIMITS
b48 whisper Lccachar dm-d-from-lcbw       expect ok   rule LIMITS
b49 whisper Lccbchar dm-d-from-lcbx       expect ok   rule LIMITS
b50 whisper Lcccchar dm-d-from-lcby       expect ok   rule LIMITS
b51 whisper Lccdchar dm-d-from-lcbz       expect ok   rule LIMITS
b52 whisper Lccechar dm-d-from-lcca       expect ok   rule LIMITS
b53 whisper Lccfchar dm-d-from-lccb       expect ok   rule LIMITS
b54 whisper Lccgchar dm-d-from-lccc       expect ok   rule LIMITS
b55 whisper Lcchchar dm-d-from-lccd       expect ok   rule LIMITS
b56 whisper Lccichar dm-d-from-lcce       expect ok   rule LIMITS
b57 whisper Lccjchar dm-d-from-lccf       expect ok   rule LIMITS
b58 whisper Lcckchar dm-d-from-lccg       expect ok   rule LIMITS
b59 whisper Lcclchar dm-d-from-lcch       expect ok   rule LIMITS
b60 whisper Lcaachar dm-d-from-lcci       expect ok   rule LIMITS
b61 whisper Lcabchar dm-d-from-lccj       expect ok   rule LIMITS
b62 whisper Lcacchar dm-d-from-lcck       expect ok   rule LIMITS
b63 whisper Lcadchar dm-d-from-lccl       expect ok   rule LIMITS
b0  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b1  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b2  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b3  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b4  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b5  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b6  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b7  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b8  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b9  heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b10 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b11 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b12 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b13 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b14 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b15 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b16 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b17 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b18 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b19 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b20 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b21 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b22 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b23 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b24 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b25 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b26 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b27 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b28 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b29 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b30 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b31 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b32 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b33 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b34 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b35 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b36 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b37 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b38 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b39 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b40 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b41 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b42 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b43 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b44 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b45 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b46 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b47 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b48 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b49 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b50 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b51 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b52 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b53 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b54 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b55 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b56 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b57 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b58 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b59 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b60 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b61 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b62 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b63 heard-in global                       expect said.count=256 said.distinct=256   within 3s   rule HEARD
b0  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b1  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b2  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b3  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b4  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b5  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b6  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b7  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b8  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b9  dm-unread                             expect pushed=4   within 3s   rule WHISPER
b10 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b11 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b12 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b13 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b14 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b15 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b16 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b17 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b18 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b19 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b20 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b21 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b22 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b23 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b24 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b25 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b26 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b27 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b28 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b29 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b30 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b31 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b32 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b33 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b34 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b35 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b36 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b37 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b38 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b39 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b40 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b41 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b42 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b43 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b44 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b45 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b46 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b47 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b48 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b49 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b50 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b51 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b52 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b53 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b54 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b55 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b56 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b57 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b58 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b59 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b60 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b61 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b62 dm-unread                             expect pushed=4   within 3s   rule WHISPER
b63 dm-unread                             expect pushed=4   within 3s   rule WHISPER

# Round 5: a Global line and a whisper from every bot.
wait 16s
b0  say global round-e-from-lcaa          expect ok   rule LIMITS
b1  say global round-e-from-lcab          expect ok   rule LIMITS
b2  say global round-e-from-lcac          expect ok   rule LIMITS
b3  say global round-e-from-lcad          expect ok   rule LIMITS
b4  say global round-e-from-lcae          expect ok   rule LIMITS
b5  say global round-e-from-lcaf          expect ok   rule LIMITS
b6  say global round-e-from-lcag          expect ok   rule LIMITS
b7  say global round-e-from-lcah          expect ok   rule LIMITS
b8  say global round-e-from-lcai          expect ok   rule LIMITS
b9  say global round-e-from-lcaj          expect ok   rule LIMITS
b10 say global round-e-from-lcak          expect ok   rule LIMITS
b11 say global round-e-from-lcal          expect ok   rule LIMITS
b12 say global round-e-from-lcam          expect ok   rule LIMITS
b13 say global round-e-from-lcan          expect ok   rule LIMITS
b14 say global round-e-from-lcao          expect ok   rule LIMITS
b15 say global round-e-from-lcap          expect ok   rule LIMITS
b16 say global round-e-from-lcaq          expect ok   rule LIMITS
b17 say global round-e-from-lcar          expect ok   rule LIMITS
b18 say global round-e-from-lcas          expect ok   rule LIMITS
b19 say global round-e-from-lcat          expect ok   rule LIMITS
b20 say global round-e-from-lcau          expect ok   rule LIMITS
b21 say global round-e-from-lcav          expect ok   rule LIMITS
b22 say global round-e-from-lcaw          expect ok   rule LIMITS
b23 say global round-e-from-lcax          expect ok   rule LIMITS
b24 say global round-e-from-lcay          expect ok   rule LIMITS
b25 say global round-e-from-lcaz          expect ok   rule LIMITS
b26 say global round-e-from-lcba          expect ok   rule LIMITS
b27 say global round-e-from-lcbb          expect ok   rule LIMITS
b28 say global round-e-from-lcbc          expect ok   rule LIMITS
b29 say global round-e-from-lcbd          expect ok   rule LIMITS
b30 say global round-e-from-lcbe          expect ok   rule LIMITS
b31 say global round-e-from-lcbf          expect ok   rule LIMITS
b32 say global round-e-from-lcbg          expect ok   rule LIMITS
b33 say global round-e-from-lcbh          expect ok   rule LIMITS
b34 say global round-e-from-lcbi          expect ok   rule LIMITS
b35 say global round-e-from-lcbj          expect ok   rule LIMITS
b36 say global round-e-from-lcbk          expect ok   rule LIMITS
b37 say global round-e-from-lcbl          expect ok   rule LIMITS
b38 say global round-e-from-lcbm          expect ok   rule LIMITS
b39 say global round-e-from-lcbn          expect ok   rule LIMITS
b40 say global round-e-from-lcbo          expect ok   rule LIMITS
b41 say global round-e-from-lcbp          expect ok   rule LIMITS
b42 say global round-e-from-lcbq          expect ok   rule LIMITS
b43 say global round-e-from-lcbr          expect ok   rule LIMITS
b44 say global round-e-from-lcbs          expect ok   rule LIMITS
b45 say global round-e-from-lcbt          expect ok   rule LIMITS
b46 say global round-e-from-lcbu          expect ok   rule LIMITS
b47 say global round-e-from-lcbv          expect ok   rule LIMITS
b48 say global round-e-from-lcbw          expect ok   rule LIMITS
b49 say global round-e-from-lcbx          expect ok   rule LIMITS
b50 say global round-e-from-lcby          expect ok   rule LIMITS
b51 say global round-e-from-lcbz          expect ok   rule LIMITS
b52 say global round-e-from-lcca          expect ok   rule LIMITS
b53 say global round-e-from-lccb          expect ok   rule LIMITS
b54 say global round-e-from-lccc          expect ok   rule LIMITS
b55 say global round-e-from-lccd          expect ok   rule LIMITS
b56 say global round-e-from-lcce          expect ok   rule LIMITS
b57 say global round-e-from-lccf          expect ok   rule LIMITS
b58 say global round-e-from-lccg          expect ok   rule LIMITS
b59 say global round-e-from-lcch          expect ok   rule LIMITS
b60 say global round-e-from-lcci          expect ok   rule LIMITS
b61 say global round-e-from-lccj          expect ok   rule LIMITS
b62 say global round-e-from-lcck          expect ok   rule LIMITS
b63 say global round-e-from-lccl          expect ok   rule LIMITS
b0  whisper Lcafchar dm-e-from-lcaa       expect ok   rule LIMITS
b1  whisper Lcagchar dm-e-from-lcab       expect ok   rule LIMITS
b2  whisper Lcahchar dm-e-from-lcac       expect ok   rule LIMITS
b3  whisper Lcaichar dm-e-from-lcad       expect ok   rule LIMITS
b4  whisper Lcajchar dm-e-from-lcae       expect ok   rule LIMITS
b5  whisper Lcakchar dm-e-from-lcaf       expect ok   rule LIMITS
b6  whisper Lcalchar dm-e-from-lcag       expect ok   rule LIMITS
b7  whisper Lcamchar dm-e-from-lcah       expect ok   rule LIMITS
b8  whisper Lcanchar dm-e-from-lcai       expect ok   rule LIMITS
b9  whisper Lcaochar dm-e-from-lcaj       expect ok   rule LIMITS
b10 whisper Lcapchar dm-e-from-lcak       expect ok   rule LIMITS
b11 whisper Lcaqchar dm-e-from-lcal       expect ok   rule LIMITS
b12 whisper Lcarchar dm-e-from-lcam       expect ok   rule LIMITS
b13 whisper Lcaschar dm-e-from-lcan       expect ok   rule LIMITS
b14 whisper Lcatchar dm-e-from-lcao       expect ok   rule LIMITS
b15 whisper Lcauchar dm-e-from-lcap       expect ok   rule LIMITS
b16 whisper Lcavchar dm-e-from-lcaq       expect ok   rule LIMITS
b17 whisper Lcawchar dm-e-from-lcar       expect ok   rule LIMITS
b18 whisper Lcaxchar dm-e-from-lcas       expect ok   rule LIMITS
b19 whisper Lcaychar dm-e-from-lcat       expect ok   rule LIMITS
b20 whisper Lcazchar dm-e-from-lcau       expect ok   rule LIMITS
b21 whisper Lcbachar dm-e-from-lcav       expect ok   rule LIMITS
b22 whisper Lcbbchar dm-e-from-lcaw       expect ok   rule LIMITS
b23 whisper Lcbcchar dm-e-from-lcax       expect ok   rule LIMITS
b24 whisper Lcbdchar dm-e-from-lcay       expect ok   rule LIMITS
b25 whisper Lcbechar dm-e-from-lcaz       expect ok   rule LIMITS
b26 whisper Lcbfchar dm-e-from-lcba       expect ok   rule LIMITS
b27 whisper Lcbgchar dm-e-from-lcbb       expect ok   rule LIMITS
b28 whisper Lcbhchar dm-e-from-lcbc       expect ok   rule LIMITS
b29 whisper Lcbichar dm-e-from-lcbd       expect ok   rule LIMITS
b30 whisper Lcbjchar dm-e-from-lcbe       expect ok   rule LIMITS
b31 whisper Lcbkchar dm-e-from-lcbf       expect ok   rule LIMITS
b32 whisper Lcblchar dm-e-from-lcbg       expect ok   rule LIMITS
b33 whisper Lcbmchar dm-e-from-lcbh       expect ok   rule LIMITS
b34 whisper Lcbnchar dm-e-from-lcbi       expect ok   rule LIMITS
b35 whisper Lcbochar dm-e-from-lcbj       expect ok   rule LIMITS
b36 whisper Lcbpchar dm-e-from-lcbk       expect ok   rule LIMITS
b37 whisper Lcbqchar dm-e-from-lcbl       expect ok   rule LIMITS
b38 whisper Lcbrchar dm-e-from-lcbm       expect ok   rule LIMITS
b39 whisper Lcbschar dm-e-from-lcbn       expect ok   rule LIMITS
b40 whisper Lcbtchar dm-e-from-lcbo       expect ok   rule LIMITS
b41 whisper Lcbuchar dm-e-from-lcbp       expect ok   rule LIMITS
b42 whisper Lcbvchar dm-e-from-lcbq       expect ok   rule LIMITS
b43 whisper Lcbwchar dm-e-from-lcbr       expect ok   rule LIMITS
b44 whisper Lcbxchar dm-e-from-lcbs       expect ok   rule LIMITS
b45 whisper Lcbychar dm-e-from-lcbt       expect ok   rule LIMITS
b46 whisper Lcbzchar dm-e-from-lcbu       expect ok   rule LIMITS
b47 whisper Lccachar dm-e-from-lcbv       expect ok   rule LIMITS
b48 whisper Lccbchar dm-e-from-lcbw       expect ok   rule LIMITS
b49 whisper Lcccchar dm-e-from-lcbx       expect ok   rule LIMITS
b50 whisper Lccdchar dm-e-from-lcby       expect ok   rule LIMITS
b51 whisper Lccechar dm-e-from-lcbz       expect ok   rule LIMITS
b52 whisper Lccfchar dm-e-from-lcca       expect ok   rule LIMITS
b53 whisper Lccgchar dm-e-from-lccb       expect ok   rule LIMITS
b54 whisper Lcchchar dm-e-from-lccc       expect ok   rule LIMITS
b55 whisper Lccichar dm-e-from-lccd       expect ok   rule LIMITS
b56 whisper Lccjchar dm-e-from-lcce       expect ok   rule LIMITS
b57 whisper Lcckchar dm-e-from-lccf       expect ok   rule LIMITS
b58 whisper Lcclchar dm-e-from-lccg       expect ok   rule LIMITS
b59 whisper Lcaachar dm-e-from-lcch       expect ok   rule LIMITS
b60 whisper Lcabchar dm-e-from-lcci       expect ok   rule LIMITS
b61 whisper Lcacchar dm-e-from-lccj       expect ok   rule LIMITS
b62 whisper Lcadchar dm-e-from-lcck       expect ok   rule LIMITS
b63 whisper Lcaechar dm-e-from-lccl       expect ok   rule LIMITS
b0  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b1  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b2  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b3  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b4  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b5  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b6  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b7  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b8  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b9  heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b10 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b11 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b12 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b13 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b14 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b15 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b16 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b17 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b18 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b19 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b20 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b21 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b22 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b23 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b24 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b25 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b26 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b27 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b28 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b29 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b30 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b31 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b32 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b33 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b34 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b35 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b36 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b37 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b38 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b39 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b40 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b41 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b42 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b43 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b44 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b45 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b46 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b47 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b48 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b49 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b50 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b51 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b52 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b53 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b54 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b55 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b56 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b57 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b58 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b59 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b60 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b61 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b62 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b63 heard-in global                       expect said.count=320 said.distinct=320   within 3s   rule HEARD
b0  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b1  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b2  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b3  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b4  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b5  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b6  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b7  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b8  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b9  dm-unread                             expect pushed=5   within 3s   rule WHISPER
b10 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b11 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b12 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b13 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b14 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b15 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b16 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b17 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b18 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b19 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b20 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b21 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b22 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b23 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b24 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b25 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b26 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b27 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b28 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b29 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b30 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b31 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b32 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b33 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b34 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b35 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b36 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b37 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b38 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b39 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b40 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b41 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b42 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b43 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b44 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b45 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b46 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b47 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b48 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b49 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b50 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b51 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b52 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b53 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b54 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b55 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b56 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b57 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b58 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b59 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b60 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b61 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b62 dm-unread                             expect pushed=5   within 3s   rule WHISPER
b63 dm-unread                             expect pushed=5   within 3s   rule WHISPER
