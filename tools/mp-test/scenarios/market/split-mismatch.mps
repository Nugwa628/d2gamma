# Economies (contract 2): softcore and hardcore characters trade in
# separate markets; the selected character decides which.
name   market-split-mismatch
about  A softcore listing is not seen by a hardcore character of another account; the same account's softcore character sees it.
rule   SPLIT   a character sees only its own economy's listings (mod, softcore or hardcore, classic or expansion) (contract 2)
bots   2
server market=on

all connect
b0  register sela sela-pass-1             expect ok
b0  char-create Selapal 3                 expect ok
b0  char-select Selapal                   expect ok
server econ-seed sela r30:1
b0  market-post r30:1 price=500           expect ok
b1  register byra byra-pass-1             expect ok
b1  char-create Byrahard 1 hardcore       expect ok
b1  char-create Byrasoft 1                expect ok
b1  char-select Byrahard                  expect ok
b1  market                                expect on=yes hardcore=yes             rule SPLIT
b1  market-search group=stackable         expect total=0                         rule SPLIT
b1  char-select Byrasoft                  expect ok
b1  market                                expect hardcore=no                     rule SPLIT
b1  market-search group=stackable         expect total=1 first.seller=Selapal    rule SPLIT
