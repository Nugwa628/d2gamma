# Game list v2 (owner 2026-10-23, lobby contract section 7): every open game is listed to
# every character, the ones it can't join greyed with the reason instead of hidden. A test
# server raises a level in a game (`dev CHARACTER level N`, saved when the bot leaves) and
# sets progression on its stdin (`dev = test`).
name   lobby-games-greyed
about  A softcore character sees a hardcore game (WrongGameKind), a full game (GameFull), a Hell game (DifficultyLocked) and a level-range game (LevelRange) listed with their reasons, and a game it can join with 0; joins follow the reasons.
rule   KIND    a game of the other hardcore / softcore kind is listed with WrongGameKind, not hidden, and refuses the join (lobby contract 7)
rule   FULL    a full game is listed with GameFull (lobby contract 7)
rule   DIFF    a game of a difficulty the character hasn't unlocked is listed with DifficultyLocked (lobby contract 7)
rule   LEVEL   a game whose level range leaves the character out is listed with LevelRange and refuses the join (lobby contract 7)
rule   OPEN    a game the character can join is listed with 0 (lobby contract 7)
bots   6
server dev=test

all connect
b0  register sofia  sofia-pass-1          expect ok
b1  register hardy  hardy-pass-1          expect ok
b2  register fulla  fulla-pass-1          expect ok
b3  register hella  hella-pass-1          expect ok
b4  register leva   leva-pass-1           expect ok
b5  register opie   opie-pass-1           expect ok
b0  char-create Sofiasc 0                 expect ok
b1  char-create Hardohc 1 hardcore        expect ok
b2  char-create Fullerr 2                 expect ok
b3  char-create Hellenn 3                 expect ok
b4  char-create Leveler 4                 expect ok
b5  char-create Openerr 5                 expect ok
server progression Hellenn 10
b0  char-select Sofiasc                   expect ok
b1  char-select Hardohc                   expect ok
b2  char-select Fullerr                   expect ok
b3  char-select Hellenn                   expect ok difficulties=7                                rule DIFF
b4  char-select Leveler                   expect ok
b5  char-select Openerr                   expect ok
b4  create-game warmup                    expect ok
server dev Leveler level 30
b4  leave-game
b4  char-list                             expect Leveler.level=30     within 15s                    rule LEVEL
b4  games                                 expect has.warmup=no        within 10s                    rule LEVEL
b4  create-game lvlgame levels=25-35      expect ok                                        rule LEVEL
b1  create-game hcgame                    expect ok
b2  create-game fullgame max=1            expect ok
b3  create-game hellgame difficulty=hell  expect ok
b5  create-game opengame                  expect ok
b0  games                                 expect has.hcgame=yes hcgame.hardcore=yes hcgame.joinable=67   within 3s   rule KIND
b0  games                                 expect has.fullgame=yes fullgame.joinable=63                   within 3s   rule FULL
b0  games                                 expect has.hellgame=yes hellgame.joinable=66                   within 3s   rule DIFF
b0  games                                 expect has.lvlgame=yes lvlgame.levels=25-35 lvlgame.joinable=120   within 3s   rule LEVEL
b0  games                                 expect has.opengame=yes opengame.joinable=0                    within 3s   rule OPEN
b1  games                                 expect has.opengame=yes opengame.hardcore=no opengame.joinable=67   within 3s   rule KIND
b0  join-game hcgame                      expect refused=WrongGameKind                                   rule KIND
b0  join-game lvlgame                     expect refused=LevelRange                                      rule LEVEL
b0  join-game opengame                    expect ok                                                      rule OPEN
