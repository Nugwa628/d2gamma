#!/usr/bin/env python3
"""Writes the realm lobby's load and soak scenarios (lobby plan L17):
tools/mp-test/scenarios/lobby/load-chat.mps and soak-presence.mps.

The scenario format has no loops, so their many bots' lines are written
here. Run it again after changing it: `py -3 tools/mp-test/gen_lobby_load.py`.
The measured load (hundreds of bots acting at once, latency per request
kind, the main server's memory) is `mp-load` (crates/mp-test/src/load.rs).
"""

import os
import string

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "scenarios", "lobby")


def letters(i, n=2):
    """Two (or `n`) lower-case letters for a number: names are letters only."""
    s = ""
    for _ in range(n):
        s = string.ascii_lowercase[i % 26] + s
        i //= 26
    return s


def row(who, verb, expect="", within="", rule=""):
    line = f"{who:<4}{verb:<38}"
    if expect:
        line += f"expect {expect}"
    if within:
        line += f"   within {within}"
    if rule:
        line += f"   rule {rule}"
    return line.rstrip()


def load_chat():
    bots = 64
    rounds = 5
    peers = 5
    acc = [f"lc{letters(i)}" for i in range(bots)]
    char = [f"Lc{letters(i)}char" for i in range(bots)]
    out = [
        "# Many players chatting at once (lobby plan L17, contract 3.1, 3.2 and 9):",
        "# 64 sessions in Global, each saying five lines there and whispering five",
        "# others, at the contract's rate limits. Written by",
        "# tools/mp-test/gen_lobby_load.py: change that, not this file.",
        "name   load-chat",
        f"about  {bots} bots: each says {rounds} Global lines (one per round, 16 s apart: 2 per 30 s) and whispers {peers} bots; every bot hears every line once within 3 s of the round, and every whisper.",
        "rule   HEARD    every Global line reaches every session in the room within 3 s, once (contract 3.1)",
        "rule   WHISPER  every whisper to an online player is pushed to it (contract 3.2)",
        "rule   LIMITS   lines at the contract's rates are never refused (contract 9)",
        f"bots   {bots}",
        "",
        "all connect",
    ]
    for i in range(bots):
        out.append(row(f"b{i}", f"register {acc[i]} {acc[i]}-pass-1", "ok"))
    for i in range(bots):
        out.append(row(f"b{i}", f"char-create {char[i]} {i % 7}", "ok"))
    for i in range(bots):
        out.append(row(f"b{i}", f"char-select {char[i]}", "ok"))
    for i in range(bots):
        out.append(row(f"b{i}", "heard-in global", "said.count=0", rule="HEARD"))
    for r in range(rounds):
        out.append("")
        out.append(f"# Round {r + 1}: a Global line and a whisper from every bot.")
        if r:
            out.append("wait 16s")
        for i in range(bots):
            out.append(row(f"b{i}", f"say global round-{letters(r, 1)}-from-{acc[i]}", "ok", rule="LIMITS"))
        for i in range(bots):
            to = char[(i + 1 + r) % bots]
            out.append(row(f"b{i}", f"whisper {to} dm-{letters(r, 1)}-from-{acc[i]}", "ok", rule="LIMITS"))
        n = bots * (r + 1)
        for i in range(bots):
            out.append(row(f"b{i}", "heard-in global", f"said.count={n} said.distinct={n}", "3s", "HEARD"))
        for i in range(bots):
            out.append(row(f"b{i}", "dm-unread", f"pushed={r + 1}", "3s", "WHISPER"))
    return "\n".join(out) + "\n"


def soak_presence():
    watchers = 6
    movers = 18
    bots = watchers + movers
    cycles = 20
    acc = [f"sp{letters(i)}" for i in range(bots)]
    char = [f"Sp{letters(i)}char" for i in range(bots)]
    mine = {w: [m for m in range(watchers, bots) if m % watchers == w] for w in range(watchers)}
    out = [
        "# Players moving between the lobby and games for ten minutes (lobby plan",
        "# L17, contract 3 and 5): friends' places and the room's user lists stay",
        "# right through the churn. Written by tools/mp-test/gen_lobby_load.py:",
        "# change that, not this file.",
        "name   soak-presence",
        f"about  {movers} bots create, join and leave games for {cycles} rounds (about 10 minutes) while {watchers} stay in the lobby: their mutual friends' places, a watched user list kept by pushes and a fresh one match where every bot is; at the end the bots log out and the lists empty.",
        "rule   FRIENDS  a mutual friend's place follows it into a game and back to the lobby (contract 5)",
        "rule   WATCHED  a watched user list kept by RoomPresence pushes stays the room's members and places (contract 3)",
        "rule   FRESH    a user list asked again matches it (contract 3)",
        "rule   GONE     players who logged out leave the lists and show offline (contract 3, 5)",
        f"bots   {bots}",
        "server limits=test",
        "server max_games=32",
        "",
        "all connect",
    ]
    for i in range(bots):
        out.append(row(f"b{i}", f"register {acc[i]} {acc[i]}-pass-1", "ok"))
    for i in range(bots):
        out.append(row(f"b{i}", f"char-create {char[i]} {i % 7}", "ok"))
    for i in range(bots):
        out.append(row(f"b{i}", f"char-select {char[i]}", "ok"))
    for i in range(bots):
        out.append(row(f"b{i}", "heard-in global", "ok"))
    out.append("")
    out.append("# Mutual friends: every mover and its watcher.")
    for w in range(watchers):
        for m in mine[w]:
            out.append(row(f"b{w}", f"friend-add {char[m]}", "ok"))
            out.append(row(f"b{m}", f"friend-add {char[w]}", "ok"))
    out.append(row("b0", "users global watch=yes", f"count={bots}", "3s", "WATCHED"))

    def check(where, games):
        """Every watcher's friends and both user lists see `where`."""
        for w in range(watchers):
            for m in mine[w]:
                if where[m] == "game":
                    exp = f"{char[m]}.where=game {char[m]}.game={games[m]}"
                else:
                    exp = f"{char[m]}.where={where[m]}"
                out.append(row(f"b{w}", f"friend-list {char[m]}", exp, "10s", "FRIENDS"))
        names = [char[m] for m in range(watchers, bots)]
        for lst, rule in (("b0", "WATCHED"), ("b1", "FRESH")):
            for k in range(0, len(names), 6):
                part = names[k:k + 6]
                exp = " ".join(f"{n}.where={where[watchers + k + j]}" for j, n in enumerate(part))
                out.append(row(lst, f"users global {' '.join(part)}", exp, "10s", rule))

    for c in range(cycles):
        out.append("")
        out.append(f"# Round {c + 1}: a third create games, a third join them, a third stay.")
        if c:
            out.append("wait 20s")
        where = {m: "lobby" for m in range(watchers, bots)}
        games = {}
        for m in range(watchers, bots):
            if (m + c) % 3 == 0:
                games[m] = f"sp{letters(m)}{letters(c)}"
                out.append(row(f"b{m}", f"create-game {games[m]}", "ok"))
                where[m] = "game"
        for m in range(watchers, bots):
            if (m + c) % 3 == 1 and m - 1 in games:
                games[m] = games[m - 1]
                out.append(row(f"b{m}", f"join-game {games[m]}", "ok"))
                where[m] = "game"
        check(where, games)
        for m in sorted(games):
            out.append(row(f"b{m}", "leave-game"))
        for m in games:
            where[m] = "lobby"
        check(where, {})

    out.append("")
    out.append("# The end: the movers log out.")
    for m in range(watchers, bots):
        out.append(row(f"b{m}", "logout", "ok", rule="GONE"))
    for w in range(watchers):
        for m in mine[w]:
            out.append(row(f"b{w}", f"friend-list {char[m]}", f"{char[m]}.where=offline", "10s", "GONE"))
    out.append(row("b0", f"users global {char[watchers]} {char[bots - 1]}", f"count={watchers} has.{char[watchers]}=no has.{char[bots - 1]}=no", "10s", "GONE"))
    out.append(row("b1", f"users global {char[watchers]} {char[bots - 1]}", f"count={watchers} has.{char[watchers]}=no has.{char[bots - 1]}=no", "10s", "GONE"))
    return "\n".join(out) + "\n"


def main():
    for name, text in (("load-chat.mps", load_chat()), ("soak-presence.mps", soak_presence())):
        with open(os.path.join(OUT, name), "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print(f"wrote {name}: {text.count(chr(10))} lines")


if __name__ == "__main__":
    main()
