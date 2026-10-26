//! Every step verb a scenario may use: the capability it needs and
//! whether the tool can drive it yet.
//!
//! A scenario is **pending** while one of the capabilities it needs is
//! missing: either the tool has no driver for a verb yet (`built: false`
//! below; the feature's wire messages aren't in the contracts or the code
//! yet), or the server doesn't serve it (its ready line lists `caps`
//! without it). Pending scenarios are reported and never fail a run. When
//! a feature lands, its lane writes the verb's driver in `runner.rs`,
//! flips `built` to true here, and the scenarios that use it run from then
//! on.

/// What a verb does to the step's flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Runs once; it may take until its `within` time.
    Action,
    /// Reads the bot's view; retried until its expectations hold or its
    /// `within` time runs out.
    Query,
}

/// One verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verb {
    /// The word in a scenario.
    pub name: &'static str,
    /// The capability it needs.
    pub cap: &'static str,
    /// Action or query.
    pub kind: Kind,
    /// The tool can drive it.
    pub built: bool,
    /// Its arguments and facts, for `--verbs`.
    pub about: &'static str,
}

const fn verb(
    name: &'static str,
    cap: &'static str,
    kind: Kind,
    built: bool,
    about: &'static str,
) -> Verb {
    Verb {
        name,
        cap,
        kind,
        built,
        about,
    }
}

use Kind::{Action, Query};

/// Every bot verb. Capabilities: `connect` (handshake, resume), `game`
/// (entering a game, seeing the others), `walk`, `accounts`,
/// `characters`, `lobby`, `chat`, `party`, `trade`, `items`, `combat`,
/// `waypoints`, `acts`, `stash`, `market`, and the realm lobby's `social`,
/// `guilds`, `friends`, `ladder`; the server-only ones are in
/// [`SERVER_CAPS`].
#[rustfmt::skip]
pub const VERBS: &[Verb] = &[
    // Connection.
    verb("connect", "connect", Action, true,
        "[build=S] [protocol=N] [tick=N] [game=ID] [trust=pinned|any|wrong]; facts client, max_players, tick"),
    verb("reconnect", "connect", Action, true,
        "comes back with the last resume token; facts client, same=yes|no"),
    verb("disconnect", "connect", Action, true, "ends the connection with a goodbye"),
    verb("stats", "connect", Query, true,
        "facts connected=yes|no, frames, ticks, delivered, bad (malformed messages), ids (count per message id)"),
    // A game, entered directly (C→S 0x67 then 0x6B), and play in it.
    verb("enter", "game", Action, true,
        "GAME [class=N] [difficulty=N]; ok once the bot's own player is assigned (refused=no-player otherwise); facts guid, x, y"),
    verb("leave", "game", Action, true, "leaves the game (C→S 0x69)"),
    verb("move", "walk", Action, true, "X Y or +DX +DY (relative to its own position): walks there"),
    verb("frames", "game", Action, true, "N: waits until N more server frames have arrived"),
    verb("see", "game", Query, true,
        "facts players (count), x, y (own), bN=yes|no, bN.x, bN.y, bN.moved=yes|no (since first seen) for every other bot"),
    verb("agree", "game", Query, true,
        "on a group: agree=yes when every bot of the group sees the same players at the same places"),
    // Accounts (register + login before anything else, owner 2026-10-19).
    verb("register", "accounts", Action, true, "ACCOUNT PASSWORD; facts ms (answer time), wait_s"),
    verb("login", "accounts", Action, true, "ACCOUNT PASSWORD; facts ms (answer time), wait_s"),
    verb("logout", "accounts", Action, true, "ends the session"),
    // Online characters, stored on the server, owned by an account.
    verb("char-create", "characters", Action, true,
        "NAME CLASS [hardcore] [classic] [ladder] (ladder: in the open season)"),
    verb("char-list", "characters", Query, true,
        "facts count, has.NAME=yes|no, NAME.level, NAME.difficulties (mask), NAME.season (its ladder season, 0 non-ladder), NAME.played=yes|no (a save was stored)"),
    verb("char-select", "characters", Action, true, "NAME; facts difficulties"),
    verb("char-delete", "characters", Action, true, "NAME PASSWORD (the account's, to confirm)"),
    // Lobby.
    verb("games", "lobby", Query, true,
        "facts count, has.NAME=yes|no, NAME.players, NAME.max, NAME.difficulty (normal|nightmare|hell), NAME.locked=yes|no, NAME.joinable (0 or an error code), NAME.hardcore=yes|no, NAME.expansion=yes|no, NAME.desc, NAME.levels (40-70, 40-, -70 or any), NAME.server (its label), NAME.node, NAME.hosted=yes|no, NAME.created=yes|no"),
    verb("create-game", "lobby", Action, true,
        "NAME [password=P] [difficulty=normal|nightmare|hell] [max=N] [desc=TEXT] [levels=MIN-MAX] [enter=no] [as=CHARACTER]: creates, attaches and enters it (enter=no: stops at the main server's answer; as: names another character in the 0x67 join); facts game_id, node (a game server's game)"),
    verb("join-game", "lobby", Action, true,
        "NAME [password=P] [enter=no] [as=CHARACTER]: joins by its name in the list, attaches and enters (as for create-game); facts game_id, difficulty, node"),
    verb("nodes", "node-choice", Query, true,
        "the main server's game-server list (NodeList, sent on entering the lobby and on change): facts count, lists (lists heard), suggested (its id, or none), has.ID=yes and ID.name, ID.location, ID.addr (where clients ping), ID.load (0-100), ID.state (open|full|draining|down) for every listed server"),
    verb("ticket", "node-games", Action, true,
        "NODE: shows the last JoinNode's ticket to that game server (Attach mode 2 on a connection of its own) and closes it; ok or the refusal"),
    verb("leave-game", "lobby", Action, true, "leaves the game (0x69, game stream ends): back in the lobby"),
    verb("chat", "chat", Action, true, "TEXT"),
    verb("heard", "chat", Query, true, "facts last (the last line heard), from"),
    verb("heard-game", "game", Query, true,
        "[TEXT..]: the server's lines in its game (0x26 with no speaker, as an upgrade's countdown); facts lines, last; with TEXT has=yes|no (a line of exactly that text), times"),
    // The realm lobby's chat (`docs/plans/server/lobby-contract.md` 3):
    // rooms global, mod, guild; DMs to a character or `*account`.
    verb("say", "social", Action, true,
        "ROOM TEXT [emote=yes]: says it in global|mod|guild (0x72); ok, or refused with the code"),
    verb("whisper", "social", Action, true,
        "NAME|*ACCOUNT TEXT: a DM (0x73); facts dm, stored=yes|no (waits for an offline player)"),
    verb("heard-in", "social", Query, true,
        "ROOM [TEXT..]: facts last (the room's last line), from, tag, what (said|emote|joined|left|notice|guild), said and said.from (the last said or emote line), lines (heard so far), backlog (lines of the hello); with TEXT has=yes|no (a line of exactly that text), times"),
    verb("dm-unread", "social", Query, true,
        "facts unread (all conversations), conversations (with unread), PEER.unread, PEER.where and PEER.last for every conversation (PEER: the account, lower case), pushed (live DMs heard), pushed.stored (waited ones pushed at the hello)"),
    verb("dm-history", "social", Query, true,
        "PEER [rows=N] [read=yes]: facts count, first, last, last.stored=yes|no, last.outgoing=yes|no, last.char (read=yes: marks it read)"),
    verb("users", "social", Query, true,
        "ROOM [NAME..] [watch=yes|no]: facts count, has.NAME=yes|no and NAME.where (offline|lobby|game|hidden) for each NAME; with watch the presence pushes keep it live (a later `users ROOM` reads it): joined, left, changed"),
    // Guilds (contract 4).
    verb("guild-create", "guilds", Action, true, "TAG NAME..: creates the guild; facts guild"),
    verb("guild-invite", "guilds", Action, true, "NAME|*ACCOUNT: invites them to its guild"),
    verb("guild-answer", "guilds", Action, true, "accept|decline [TAG]: answers the newest invite (or TAG's)"),
    verb("guild-leave", "guilds", Action, true, "leaves its guild"),
    verb("guild-kick", "guilds", Action, true, "NAME: kicks a member"),
    verb("guild-promote", "guilds", Action, true, "NAME: makes a member an officer"),
    verb("guild-demote", "guilds", Action, true, "NAME: makes an officer a member"),
    verb("guild-leader", "guilds", Action, true, "NAME: hands the lead over"),
    verb("guild-disband", "guilds", Action, true, "TAG: disbands its guild (the tag confirms)"),
    verb("guild-motd", "guilds", Action, true, "TEXT: sets the message of the day"),
    verb("guild", "guilds", Query, true,
        "[TAG]: facts in=yes|no, tag, name, motd, members, leader, my_rank (member|officer|leader), NAME.rank and NAME.where for every member (NAME: the character), invites, notice (the last notice's kind: invited|joined|left|kicked|promoted|demoted|leader|disbanded|motd), notice.who, notice.by, notices; with TAG the public facts of that guild (in: the bot is a member), refused=BadGuildName when no guild has it"),
    // Friends, ignores and privacy (contract 5).
    verb("friend-add", "friends", Action, true, "NAME|*ACCOUNT: adds them (0xFA op 0); ok, or refused with the code"),
    verb("friend-remove", "friends", Action, true, "NAME|*ACCOUNT"),
    verb("friend-ignore", "friends", Action, true, "NAME|*ACCOUNT: their room lines and DMs stop reaching it"),
    verb("friend-unignore", "friends", Action, true, "NAME|*ACCOUNT"),
    verb("friend-privacy", "friends", Action, true,
        "[whispers=everyone|friends|nobody] [where=friends|nobody]: a choice it doesn't name stays"),
    verb("friend-list", "friends", Query, true,
        "[ignored=yes] [NAME..]: facts count, has.NAME=yes|no, NAME.where (offline|lobby|game|hidden), NAME.mutual=yes|no, NAME.game, NAME.game_id, NAME.joinable, NAME.seen=yes|no for every row (under its character) and each NAME (a character or account, any case); notice (the last notice's kind: online|offline|entered|left|added), notice.from, notice.where, notices (heard so far)"),
    verb("friend-join", "friends", Action, true,
        "NAME [password=P] [enter=no] [as=CHARACTER]: joins the game the friends list shows for NAME by its id (0x46), then attaches and enters as join-game"),
    // The ladder (contract 6).
    verb("ladder", "ladder", Query, true,
        "[season=N] [hc=yes] [classic=yes] [class=N] [from=N] [rows=N] [me=yes]: the seasons, then a page of season N (the current one without it); facts seasons, current (a season or none), season.N (its name), season.N.test, season.N.ended, total, first_rank, rows, first, first.level, first.exp, my_rank, pinned=yes|no, NAME.rank, NAME.level, NAME.class, NAME.dead=yes|no, NAME.mine, NAME.me, NAME.online, NAME.tag; a refusal's code"),
    verb("ladder-find", "ladder", Query, true,
        "NAME [season=N] [hc=yes] [classic=yes] [class=N]: the page with NAME in season N (the current one without it); facts rank, first_rank, total and the page's as for ladder; refused=UnknownPlayer when it isn't there"),
    // Play together.
    verb("party-invite", "party", Action, false, "bN"),
    verb("party-accept", "party", Action, false, "bN"),
    verb("party", "party", Query, false, "facts members, bN=yes|no"),
    verb("trade", "trade", Action, false, "bN: asks bN to trade"),
    verb("trade-accept", "trade", Action, false, "accepts the open trade request"),
    verb("trade-offer", "trade", Action, false, "ITEM: puts an item in the trade window"),
    verb("trade-confirm", "trade", Action, false, "confirms the trade"),
    verb("inventory", "items", Query, true,
        "[ITEM...]: facts count, has.ITEM=yes|no, ITEM.n (how many) and ITEM.cursor=yes|no (ITEM an item code, every one it has seen or the step names; items it holds: inventory, belt, cursor, worn)"),
    verb("drop", "items", Action, true, "ITEM: drops it on the ground (to the cursor, then down); ok once it lies there"),
    verb("pickup", "items", Action, true, "ITEM: walks to the nearest one on the ground and picks it up; ok once it holds it"),
    verb("ground", "items", Query, true, "facts count, has.ITEM=yes|no (items it sees on the ground)"),
    // The online shared stash (plan N28): the account's, on the main server.
    verb("stash-put", "stash", Action, true,
        "ITEM [as=stack] [answer=no]: the item it holds goes to the shared stash (0xA2; as=stack: to its slot); ok once stored, else refused with the code (answer=no: ok once sent); facts unit, count"),
    verb("stash-send", "stash", Action, true,
        "[ITEM...] [answer=no]: every unit it holds of those codes (all without one) goes to its slot in one request (0xA5, the send button); facts units, count (moved), left"),
    verb("stash-take", "stash", Action, true,
        "ITEM [to=cursor] [answer=no]: the first stash item whose name or code holds ITEM comes to the inventory, or the cursor (0xA3); ok once there (answer=no: once sent); facts item, count"),
    verb("stash-take-stack", "stash", Action, true,
        "CODE N|all [answer=no]: N (or all) of slot CODE come to the inventory as plain items (0xA3); ok once there; facts count"),
    verb("stash", "stash", Query, true,
        "[ITEM...]: facts gold, items (first page), has.ITEM=yes|no, ITEM.count and slot.ITEM (its slot's count) for the named items and every item code it has seen"),
    // The market (plan K1c): the main server's, on the selected
    // character's economy. Prices: `G`, `G+CODE:N..` or `CODE:N..`.
    verb("market", "market", Query, true,
        "a hello: facts on, why (on|switch|no-character), last_req, hardcore, listings, deliveries, offers_to_me, my_offers, max_listings, max_offer_items, searches_per_min, sale_fee_pct, stackables, and the pushes so far: notices, notice (the last one's kind), notice.kinds, notice.who, notice.waiting, pushed=yes|no"),
    verb("market-search", "market", Query, true,
        "[TEXT..] [group=G] [type=T] [sort=price] [facets=yes]: facts total, rows, first.seller, first.group, first.goods, first.mine, first.price, first.offers, sellers; with facets groups, group.G, type.T"),
    verb("market-pick", "market", Action, true,
        "[TEXT..] [group=G] [type=T]: the first row of the search not its own becomes the picked listing; facts listing, seller, price"),
    verb("market-get", "market", Query, true, "the picked listing's details: facts state, offers, price, my_offer"),
    verb("market-buy", "market", Action, true,
        "buys the picked listing at the price it was picked at (PriceChanged if it changed since; ListingGone if it is gone)"),
    verb("market-post", "market", Action, true,
        "CODE:N|ITEM [price=P] [mode=price|offers|both] [days=N] [note=TEXT] [hidden=yes]: lists a stackable count or a stash item; facts id"),
    verb("market-offer", "market", Action, true, "price=P: an offer on the picked listing; facts id"),
    verb("market-answer", "market", Action, true,
        "accept|decline|counter|withdraw [price=P] [side=mine|made]: answers the newest live offer on its listings (side=made: of its own)"),
    verb("market-offers", "market", Query, true,
        "[made=yes] [all=yes]: facts count, first.state, first.offered, first.counter, first.buyer"),
    verb("market-shop", "market", Query, true,
        "[tab=shown|hidden|sold|expired]: facts rows, shown, hidden, sold, expired, slots, first.state, first.price, first.sold_to, first.note"),
    verb("market-edit", "market", Action, true,
        "cancel|hide|show|relist DAYS|note TEXT|reprice P [tab=T]: one bulk edit of the tab's first page; facts rows, rows_ok, row_refused"),
    verb("market-deliveries", "market", Query, true, "facts count, first.reason, first.goods, first.about, reasons"),
    verb("market-claim", "market", Action, true, "claims every delivery into the stash; facts rows, rows_ok, row_refused"),
    verb("market-prices", "market", Query, true,
        "CODE|picked [days=N]: facts series, sold, listed_now, recent, s0.sold, s0.buckets, s0.median"),
    verb("market-flood", "market", Action, true,
        "N: N searches back to back; facts ok, limited (RateLimited answers), wait_s"),
    verb("market-repeat", "market", Action, true,
        "sends the last change request again with its number; facts repeated"),
    verb("attack", "combat", Action, true,
        "MONSTER (as a dev spawn named it): every bot of the step attacks it until it sees it die; ok once it attacked at least once; facts attacks"),
    verb("monster", "combat", Query, true,
        "MONSTER (as a dev spawn named it): facts seen=yes|no, life (0..128), dead=yes|no"),
    verb("waypoint", "waypoints", Action, true,
        "AREA (a name such as cold-plains, or a level id): travels by the nearest waypoint; ok once there"),
    verb("act", "acts", Action, true, "N: travels by waypoint to act N's town; ok once there"),
    verb("area", "game", Query, true, "facts area (a name such as cold-plains, or the level id), level, act"),
];

/// The v2 setup verbs (plan N1): they start their line and act on the
/// setup's processes. `setup`: any v2 setup (the tool serves it);
/// `hosting`: a hosting process (contracts 11.12).
#[rustfmt::skip]
pub const TOPO_VERBS: &[Verb] = &[
    verb("kill-node", "setup", Action, true, "ID: kills that game-server process at once (a crash)"),
    verb("stop-node", "setup", Action, true, "ID: stops it cleanly (SIGTERM where there is one)"),
    verb("start-node", "setup", Action, true,
        "ID [version=V] [update=off]: starts it again (as version V: D2RUST_TEST_VERSION; update=off: without its updater) and waits for its ready line; one that exits for a swap first starts again as the staged version"),
    verb("pause-node", "setup", Action, true,
        "ID: freezes the process (SIGSTOP; Unix only): it answers nothing until resumed or killed"),
    verb("resume-node", "setup", Action, true, "ID: lets a paused process run on (SIGCONT)"),
    verb("cut-link", "setup", Action, true, "ID [on|off]: cuts (or restores) only its link to the main server"),
    verb("restart-realm", "setup", Action, true,
        "[version=V]: stops the main server cleanly and starts it again; game servers reach it at the same address. With version=V, as server-ctl's upgrade job: once its upgrade is at phase swap (logs/upgrade.json), it starts again as version V"),
    verb("node", "setup", Query, true,
        "ID: facts running, linked, links (links since this start), starts, node (its id), closed (reason), public (outside address), nat, cut, host, chars (character files in its folder), swaps (starts again after a swap), version (the one it was told to run), refusal (the code of the main server's last refusal)"),
    verb("host-sleep", "hosting", Action, true, "NAME: the host's PC goes to sleep (`sleep` on the hosting process's stdin)"),
    verb("host-quit", "hosting", Action, true, "NAME: the host quits (`stop`): the process exits by itself"),
    verb("host-client-crash", "hosting", Action, true,
        "NAME: the host's game client crashes (the hosting process's stdin closes)"),
];

/// Step words that aren't verbs: `on-node ID` on a `create-game` step
/// (where the game runs, `CreateGame`'s placement) needs `node-choice`.
#[rustfmt::skip]
pub const MODIFIERS: &[Verb] = &[
    verb("on-node", "node-choice", Action, true,
        "ID|auto|pick:ID|host, on create-game: on that official game server, Automatic (the main server chooses), Automatic with the client's pick, or Host on my PC"),
];

/// Capabilities only the server can say it has: a scenario that needs
/// one runs only on a server whose ready line lists it. `persist`
/// (accounts and characters kept across a restart), `admin` (offline
/// admin commands: `server admin ARGS`), `dev` (dev commands for a
/// character on a test server: `server dev CHARACTER COMMAND`), `mods`;
/// a v2 setup's `nodes`, `node-games` (its game servers play the main
/// server's games: seat offers and `JoinNode`, plan N4), `hosting` and
/// `hostile` (a hostile test build's game servers list it), and `upgrade`
/// (the upgrade's countdown: `server upgrade VERSION MINUTES MINUTE_MS`
/// and `server upgrade-cancel` on a test server).
pub const SERVER_CAPS: &[&str] = &[
    "persist",
    "admin",
    "dev",
    "mods",
    "nodes",
    "node-games",
    "hosting",
    "hostile",
    "upgrade",
];

/// Capabilities the tool can drive: every built verb's, plus the server
/// steps the runner does itself.
pub fn driven() -> std::collections::BTreeSet<&'static str> {
    let mut s: std::collections::BTreeSet<&str> = VERBS
        .iter()
        .chain(TOPO_VERBS)
        .filter(|v| v.built)
        .map(|v| v.cap)
        .collect();
    s.extend(["persist", "admin", "dev", "hostile", "upgrade"]);
    s
}

/// The v2 setup verb named `name`.
pub fn find_topo(name: &str) -> Option<&'static Verb> {
    TOPO_VERBS.iter().find(|v| v.name == name)
}

/// The capability a server step needs, if any.
pub fn server_step_cap(verb: &str) -> Option<&'static str> {
    match verb {
        "admin" => Some("admin"),
        "dev" | "progression" => Some("dev"),
        "econ-seed" | "econ-clock" => Some("market"),
        "season-start" | "season-end" | "ladder-seed" => Some("ladder"),
        "notice" | "mute" | "unmute" | "social-clock" => Some("social"),
        "upgrade" | "upgrade-cancel" => Some("upgrade"),
        _ => None,
    }
}

/// The verb named `name`.
pub fn find(name: &str) -> Option<&'static Verb> {
    VERBS.iter().find(|v| v.name == name)
}

/// Extra capabilities a verb's arguments ask for (`connect game=X`, a
/// hello for another game, needs `mods`: the handshake has a game id from
/// wire version 4 on).
pub fn arg_caps(verb: &str, args: &[String]) -> Vec<&'static str> {
    match verb {
        "connect" | "reconnect" if args.iter().any(|a| a.starts_with("game=")) => vec!["mods"],
        _ => Vec::new(),
    }
}
