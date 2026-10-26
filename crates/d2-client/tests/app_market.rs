//! The market window on the real game over the control channel
//! (`play --offscreen --control stdio`), against the in-process fake
//! market (`D2RUST_MARKET=fake`): Ctrl+I opens it and not the inventory;
//! the filter counts follow a quality filter; the grid / table switch;
//! Buy opens the confirm with the exact price and the confirm buys; the
//! Offers tab answers and counters, the Deliveries tab groups by sale and
//! claims only at the stash, and Offer... builds a bundle. In
//! single player (no fake) Ctrl+I opens nothing and the market says
//! "Online only".
//!
//! Real data (`D2_GAME_DIR`) and a GPU, so `#[ignore]`. The window is
//! hidden and never takes focus; nothing reads or moves the real mouse.
//! Shots show game art: they go to the build's own temporary folder, or
//! to `D2RUST_MARKET_SHOTS` when set.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::Value;

/// A `play` game on the channel.
struct Game {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    errors: PathBuf,
}

impl Game {
    /// `tag` names the run's own save folder; `fake` turns the fake market
    /// on.
    fn start(tag: &str, fake: bool) -> Self {
        std::fs::create_dir_all(tmp().join(tag)).expect("the save folder");
        let who = ["--new", "sorceress", "Mkt", "--seed", "7", "--save-dir"];
        let mut args: Vec<std::ffi::OsString> = who.iter().map(Into::into).collect();
        args.push(tmp().join(tag).into());
        Self::launch(tag, fake, &args)
    }

    /// The game of the character saved at `save`, the fake market on; the
    /// script walks it to the town's stash and opens it.
    fn load(tag: &str, save: &std::path::Path) -> Self {
        let args = [
            "--save".into(),
            save.as_os_str().to_owned(),
            "--input".into(),
            "wait 25; clickunit 2 267; wait 1".into(),
        ];
        Self::launch(tag, true, &args)
    }

    fn launch(tag: &str, fake: bool, who: &[std::ffi::OsString]) -> Self {
        let dir = std::env::var_os("D2_GAME_DIR").expect("D2_GAME_DIR");
        let errors = tmp().join(format!("{tag}.stderr.log"));
        std::fs::create_dir_all(tmp()).expect("the run folder");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_D2Gamma-Game"));
        cmd.args(["play", "--game-dir"])
            .arg(dir)
            .args(who)
            .arg("--no-sound")
            .args(["--offscreen", "--size", "1280x720", "--control", "stdio"])
            .env_remove("D2RUST_MARKET")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(&errors).expect("the stderr log"));
        if fake {
            cmd.env("D2RUST_MARKET", "fake");
        }
        let mut child = cmd.spawn().expect("the game starts");
        let input = child.stdin.take().expect("stdin");
        let output = BufReader::new(child.stdout.take().expect("stdout"));
        let mut g = Game {
            child,
            input,
            output,
            errors,
        };
        assert_eq!(g.reply()["k"], "hello");
        g
    }

    /// The next protocol line; the game's own log lines are skipped.
    fn reply(&mut self) -> Value {
        loop {
            let mut line = String::new();
            let n = self.output.read_line(&mut line).expect("read");
            if n == 0 {
                let errors = std::fs::read_to_string(&self.errors).unwrap_or_default();
                let lines: Vec<&str> = errors.lines().collect();
                let tail = lines[lines.len().saturating_sub(30)..].join("\n");
                panic!("the game ended without a reply\nstderr:\n{tail}");
            }
            if line.starts_with('{') {
                return serde_json::from_str(&line).expect("a JSON reply");
            }
        }
    }

    fn send(&mut self, line: &str) -> Value {
        writeln!(self.input, "{line}").expect("write");
        self.input.flush().expect("flush");
        let r = self.reply();
        assert_ne!(r["k"], "error", "`{line}`: {r}");
        r
    }

    /// `line`, then a few passes for it to show.
    fn act(&mut self, line: &str) {
        self.send(line);
        self.send("step 3");
    }

    fn market(&mut self) -> Value {
        self.send("state")["client"]["market"].clone()
    }

    fn open_states(&mut self) -> Vec<u64> {
        self.send("state")["client"]["open"]
            .as_array()
            .expect("open")
            .iter()
            .filter_map(Value::as_u64)
            .collect()
    }

    fn ids(&mut self) -> Vec<String> {
        self.send("ui")["ids"]
            .as_array()
            .expect("ids")
            .iter()
            .filter_map(|e| e["id"].as_str().map(str::to_owned))
            .collect()
    }

    fn shot(&mut self, name: &str) {
        let dir = std::env::var_os("D2RUST_MARKET_SHOTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| tmp().join("shots"));
        std::fs::create_dir_all(&dir).expect("the shot folder");
        let path = dir.join(format!("{name}.png"));
        let s = self.send(&format!("shot {}", path.display()));
        assert_eq!(s["k"], "shot", "{s}");
    }

    fn quit(mut self) {
        self.send("quit");
        drop(self.input);
        let status = self.child.wait().expect("the game exits");
        assert!(status.success(), "exit {status}");
    }
}

fn tmp() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("market-{}", std::process::id()))
}

/// The inventory's UI state.
const INVENTORY: u64 = 1;

#[test]
#[ignore = "real data: needs D2_GAME_DIR and a GPU"]
fn the_market_window_opens_filters_switches_and_buys_on_the_fake() {
    let mut g = Game::start("online", true);
    g.send("step-ticks 20");
    assert_eq!(g.market()["online"], true);
    assert_eq!(g.market()["open"], false);
    g.act("key ctrl+I");
    let m = g.market();
    assert_eq!(m["open"], true, "{m}");
    assert!(
        !g.open_states().contains(&INVENTORY),
        "Ctrl+I opened the inventory"
    );
    let ids = g.ids();
    for id in [
        "market.tab(Browse)",
        "market.filter.quality(Unique)",
        "market.result(3)",
    ] {
        assert!(ids.iter().any(|i| i == id), "{id}");
    }
    g.shot("market-grid");
    // The quality filter: the results are its count.
    let all = m["total"].as_u64().unwrap();
    let uniques = m["groups"]["Unique"].as_u64().unwrap();
    assert!(uniques > 0 && uniques < all, "{m}");
    g.act("click ui:market.filter.quality(Unique)");
    assert_eq!(g.market()["total"].as_u64(), Some(uniques));
    g.act("click ui:market.filter.quality(Unique)");
    assert_eq!(g.market()["total"].as_u64(), Some(all));
    // Grid / table.
    g.act("click ui:market.view(table)");
    assert_eq!(g.market()["view"], "table");
    assert!(g.ids().iter().any(|i| i == "market.sort(price)"));
    g.act("click ui:market.sort(price)");
    g.shot("market-table");
    // Buy: the first listing with a price; the confirm; bought.
    let mut picked = false;
    for i in 0..10 {
        g.act(&format!("click ui:market.result({i})"));
        if g.ids().iter().any(|i| i == "market.buy") {
            picked = true;
            break;
        }
    }
    assert!(picked, "a listing with a price");
    let listing = g.market()["selected"].clone();
    g.act("click ui:market.buy");
    let m = g.market();
    assert_eq!(m["confirm"]["listing"], listing, "{m}");
    assert_eq!(m["confirm"]["buyer_fee"], 0);
    g.shot("market-confirm");
    g.act("click ui:market.confirm");
    let m = g.market();
    let note = m["note"].as_str().unwrap_or_default().to_owned();
    assert!(note.contains("Deliveries"), "{m}");
    assert!(m["confirm"].is_null() && m["selected"].is_null(), "{m}");
    g.shot("market-bought");
    // Ctrl+I again closes; I alone is the inventory's still.
    g.act("key ctrl+I");
    assert_eq!(g.market()["open"], false);
    g.act("key I");
    assert!(g.open_states().contains(&INVENTORY));
    g.quit();
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR and a GPU"]
fn in_single_player_ctrl_i_opens_nothing() {
    let mut g = Game::start("single", false);
    g.send("step-ticks 20");
    let m = g.market();
    assert_eq!(m["online"], false);
    assert_eq!(m["why_not"], "Online only");
    g.act("key ctrl+I");
    assert_eq!(g.market()["open"], false);
    assert!(!g.open_states().contains(&INVENTORY));
    assert!(!g.ids().iter().any(|i| i.starts_with("market.")));
    g.quit();
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR and a GPU"]
fn the_offers_and_deliveries_tabs_on_the_fake() {
    let mut g = Game::start("trade", true);
    g.send("step-ticks 20");
    g.act("key ctrl+I");
    assert_eq!(g.market()["open"], true);
    // Offers: offers on the player's listings and the player's own.
    g.act("click ui:market.tab(Offers)");
    let m = g.market();
    assert_eq!(m["tab"], "Offers", "{m}");
    assert_eq!(m["offers"]["mine"], 3, "{m}");
    assert_eq!(m["offers"]["made"], 2, "{m}");
    g.act("click ui:market.offers.mine(0)");
    let ids = g.ids();
    for id in [
        "market.offer.accept",
        "market.offer.counter",
        "market.offer.decline",
    ] {
        assert!(ids.iter().any(|i| i == id), "{id}");
    }
    assert!(!ids.iter().any(|i| i == "market.offer.withdraw"));
    g.shot("market-offers");
    // Counter...: the form starts as the offer, items kept.
    g.act("click ui:market.offer.counter");
    let m = g.market();
    assert_eq!(m["form"]["counter"], true, "{m}");
    assert_eq!(m["form"]["items"], 1, "{m}");
    g.act("click ui:market.form.more(1)");
    g.shot("market-counter");
    g.act("click ui:market.form.send");
    assert_eq!(g.market()["form"]["review"], true);
    g.shot("market-counter-review");
    g.act("click ui:market.form.send");
    let m = g.market();
    assert!(m["form"].is_null(), "{m}");
    let note = m["note"].as_str().unwrap_or_default().to_owned();
    assert!(note.starts_with("Countered"), "{m}");
    // The player's countered offer: Accept and Withdraw.
    g.act("click ui:market.offers.made(0)");
    let ids = g.ids();
    assert!(ids.iter().any(|i| i == "market.offer.withdraw"));
    g.shot("market-offers-made");
    // Deliveries: grouped by sale; away from the stash nothing is claimed.
    // The trading side's 5, and the goods of My shop's made-up cancelled
    // and expired listings (2).
    g.act("click ui:market.tab(Deliveries)");
    let m = g.market();
    assert_eq!(m["deliveries"], 7, "{m}");
    assert_eq!(m["delivery_lines"], 6, "{m}");
    assert_eq!(m["mark"], 7, "{m}");
    assert_eq!(m["at_stash"], false, "{m}");
    g.act("click ui:market.deliveries.claim_all");
    let m = g.market();
    assert_eq!(m["deliveries"], 7, "{m}");
    let note = m["note"].as_str().unwrap_or_default().to_owned();
    assert!(note.starts_with("Claim at your stash"), "{m}");
    g.shot("market-deliveries");
    // Offer... from the browser: a currency and a stash item (the gold box
    // takes typing, which the channel does not send here).
    g.act("click ui:market.tab(Browse)");
    let mut picked = false;
    for i in 0..30 {
        g.act(&format!("click ui:market.result({i})"));
        if g.ids().iter().any(|i| i == "market.offer") {
            picked = true;
            break;
        }
    }
    assert!(picked, "a listing that takes offers");
    g.act("click ui:market.offer");
    assert_eq!(g.market()["form"]["counter"], false);
    g.act("click ui:market.form.more(0)");
    g.act("click ui:market.form.item(1)");
    let m = g.market();
    assert_eq!(m["form"]["items"], 1, "{m}");
    g.shot("market-offer-form");
    g.act("click ui:market.form.send");
    g.act("click ui:market.form.send");
    let m = g.market();
    let note = m["note"].as_str().unwrap_or_default().to_owned();
    assert!(note.starts_with("Offer made"), "{m}");
    g.quit();
}

/// The stash's UI state.
const STASH: u64 = 0x19;

/// A copy of the folder `from` (the seeded character and its shared
/// stash) in the run's own folder `tag`.
fn copy_folder(from: &std::path::Path, tag: &str) -> PathBuf {
    let to = tmp().join(tag);
    std::fs::create_dir_all(&to).expect("the save folder");
    for e in std::fs::read_dir(from).expect("the seed folder").flatten() {
        if e.file_type().is_ok_and(|t| t.is_file()) {
            std::fs::copy(e.path(), to.join(e.file_name())).expect("a seed file");
        }
    }
    to
}

/// Selling from the shared stash's Market category (an item picked up
/// from page I onto the Sell slot, a price in currencies, posted into
/// escrow) and My shop's four tabs with a bulk select, on the fake. The
/// character is the one `app_shared_stash::seed_a_stash_for_the_shots`
/// writes to `D2_STASH_SEED_DIR`; skipped without it.
#[test]
#[ignore = "real data: needs D2_GAME_DIR, D2_STASH_SEED_DIR and a GPU"]
fn selling_from_the_stash_and_my_shop_on_the_fake() {
    let Some(seed) = std::env::var_os("D2_STASH_SEED_DIR").map(PathBuf::from) else {
        return;
    };
    let dir = copy_folder(&seed, "sell");
    let mut g = Game::load("sell", &dir.join("Stasher.d2s"));
    // The script's click walks to the stash and opens it.
    let mut opened = false;
    for _ in 0..12 {
        g.send("step-ticks 25");
        if g.open_states().contains(&STASH) {
            opened = true;
            break;
        }
    }
    assert!(opened, "the stash opened");
    g.act("click ui:stash.tab(I)");
    // The Market button: the stash's Market category and the form.
    g.act("click ui:stash.market");
    let ids = g.ids();
    for id in [
        "market.sell.slot",
        "market.sell.mode(both)",
        "stash.page.slot(0,0)",
    ] {
        assert!(ids.iter().any(|i| i == id), "{id}");
    }
    assert_eq!(g.market()["open"], false);
    g.shot("sell-empty");
    // Page I's first item picked up and dropped on the Sell slot.
    g.act("click ui:stash.page.slot(0,0)");
    assert!(g.market()["sell"]["held"].is_u64());
    g.act("click ui:market.sell.slot");
    let m = g.market();
    let item = m["sell"]["item"].as_u64().expect("an item on the slot");
    // Price + offers, for gold and two currencies.
    g.act("click ui:market.sell.mode(both)");
    g.act("click ui:market.sell.gold");
    for k in ["2", "5", "0", "0", "0"] {
        g.act(&format!("key {k}"));
    }
    g.act("click ui:market.sell.add_coin");
    g.act("click ui:market.sell.pick(r20)");
    g.act("click ui:market.sell.add_coin");
    g.act("click ui:market.sell.pick(r23)");
    g.act("click ui:market.sell.coin(r23)");
    g.act("click ui:market.sell.days(3)");
    g.shot("sell-form");
    // Posted: into escrow, the page leaves it out.
    let before = g.market()["active_listings"].as_u64().unwrap_or(0);
    g.act("click ui:market.sell.post");
    let m = g.market();
    assert_eq!(m["sell"]["escrowed"][0].as_u64(), Some(item), "{m}");
    assert!(m["sell"]["item"].is_null());
    assert_eq!(m["active_listings"].as_u64(), Some(before + 1), "{m}");
    g.shot("sell-posted");
    // My shop: the posted listing, the made-up ones and the trading
    // side's three (8 shown).
    g.act("click ui:market.sell.shop");
    let m = g.market();
    assert_eq!(m["open"], true, "{m}");
    assert_eq!(m["tab"], "MyShop");
    assert_eq!(m["shop"]["counts"][0].as_u64(), Some(8), "{m}");
    assert_eq!(m["shop"]["rows"].as_u64(), Some(8));
    g.shot("shop-shown");
    g.act("click ui:market.shop.all");
    assert_eq!(g.market()["shop"]["selected"].as_u64(), Some(8));
    let ids = g.ids();
    for id in [
        "market.shop.set_price",
        "market.shop.hide",
        "market.shop.relist",
    ] {
        assert!(ids.iter().any(|i| i == id), "{id}");
    }
    g.shot("shop-shown-selected");
    g.act("click ui:market.shop.note");
    g.shot("shop-note-box");
    g.act("click ui:market.shop.box.cancel");
    for tab in ["Hidden", "Sold", "Expired"] {
        g.act(&format!("click ui:market.shop.tab({tab})"));
        assert_eq!(g.market()["shop"]["tab"], tab);
        g.shot(&format!("shop-{}", tab.to_lowercase()));
    }
    // The browser's table: every header whole, LISTED with its sort mark.
    g.act("click ui:market.tab(Browse)");
    g.act("click ui:market.view(table)");
    g.act("click ui:market.sort(posted)");
    g.shot("browse-table-header");
    g.quit();
}

/// Opens the price check of the first listing of quality `q` (the filter
/// alone on) and returns its `prices` state.
fn check_first(g: &mut Game, q: &str) -> Value {
    g.act(&format!("click ui:market.filter.quality({q})"));
    g.act("click ui:market.result(0)");
    g.act("click ui:market.price_check");
    let p = g.market()["prices"].clone();
    assert_eq!(p["open"], true, "{p}");
    assert_eq!(p["waiting"], false, "{p}");
    p
}

/// The point ids of the price graph on screen.
fn points(g: &mut Game) -> Vec<String> {
    g.ids()
        .into_iter()
        .filter(|i| i.starts_with("market.price.point("))
        .collect()
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR and a GPU"]
fn the_price_check_graphs_each_currency_on_the_fake() {
    let mut g = Game::start("prices", true);
    g.send("step-ticks 20");
    g.act("key ctrl+I");
    assert_eq!(g.market()["open"], true);
    // A unique: gold, one rune and bundles, each its own strip.
    let p = check_first(&mut g, "Unique");
    assert_eq!(p["days"], 30);
    assert_eq!(p["enough"], true, "{p}");
    let shapes: Vec<&str> = p["series"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["sold"].as_u64() > Some(0))
        .filter_map(|s| s["shape"].as_str())
        .collect();
    assert!(shapes.contains(&"one") && shapes.contains(&"bundle"), "{p}");
    assert!(p["strips"].as_u64() >= Some(3), "{p}");
    let pts = points(&mut g);
    assert!(!pts.is_empty());
    g.shot("price-mixed");
    // A point's tip: the mouse over the newest point.
    let ids = g.send("ui")["ids"].clone();
    let last = ids
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == pts[pts.len() - 1].as_str())
        .cloned()
        .expect("the point's element");
    let (x, y) = (
        last["at"][0].as_i64().unwrap(),
        last["at"][1].as_i64().unwrap(),
    );
    g.act(&format!("move {x} {y}"));
    g.shot("price-mixed-hover");
    // 90 days.
    g.act("click ui:market.price.days(90)");
    let p = g.market()["prices"].clone();
    assert_eq!(p["days"], 90);
    g.shot("price-mixed-90d");
    g.act("click ui:market.price.close");
    assert_eq!(g.market()["prices"]["open"], false);
    g.act("click ui:market.filter.quality(Unique)");
    // A set item: one rune only, one strip.
    let p = check_first(&mut g, "Set");
    assert_eq!(p["strips"], 1, "{p}");
    g.shot("price-one-currency");
    g.act("click ui:market.price.close");
    g.act("click ui:market.filter.quality(Set)");
    // A magic item: too few sales to draw.
    let p = check_first(&mut g, "Magic");
    assert_eq!(p["enough"], false, "{p}");
    assert!(points(&mut g).is_empty());
    g.shot("price-sparse");
    // Esc closes the box, then the window.
    g.act("key ESC");
    assert_eq!(g.market()["prices"]["open"], false);
    assert_eq!(g.market()["open"], true);
    g.quit();
}

/// The screen point of UI element `id`.
fn at_of(g: &mut Game, id: &str) -> (i64, i64) {
    let ids = g.send("ui")["ids"].clone();
    let e = ids
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no {id} on screen"));
    (e["at"][0].as_i64().unwrap(), e["at"][1].as_i64().unwrap())
}

/// The New listing form's price check, on the fake: the Sell slot's item
/// checked as it lands, its "Similar active / last sold" line and the
/// line's tip, and Price check's graph over the form. The character is
/// the seeded one `selling_from_the_stash_and_my_shop_on_the_fake` uses;
/// skipped without it.
#[test]
#[ignore = "real data: needs D2_GAME_DIR, D2_STASH_SEED_DIR and a GPU"]
fn the_sell_form_price_checks_its_slot_item_on_the_fake() {
    let Some(seed) = std::env::var_os("D2_STASH_SEED_DIR").map(PathBuf::from) else {
        return;
    };
    let dir = copy_folder(&seed, "sell-prices");
    let mut g = Game::load("sell-prices", &dir.join("Stasher.d2s"));
    let mut opened = false;
    for _ in 0..12 {
        g.send("step-ticks 25");
        if g.open_states().contains(&STASH) {
            opened = true;
            break;
        }
    }
    assert!(opened, "the stash opened");
    g.act("click ui:stash.tab(I)");
    g.act("click ui:stash.market");
    // Page I's first item onto the Sell slot: checked as it lands.
    g.act("click ui:stash.page.slot(0,0)");
    g.act("click ui:market.sell.slot");
    g.send("step-ticks 2");
    let m = g.market();
    let item = m["sell"]["item"].as_u64().expect("an item on the slot");
    let p = m["prices"].clone();
    assert_eq!(p["stash"].as_u64(), Some(item), "{p}");
    assert_eq!(p["open"], false, "the line alone: no box");
    let line = p["similar"].as_str().expect("the line").to_owned();
    assert!(line.starts_with("Similar active: "), "{line}");
    assert!(line.contains("last sold: "), "{line}");
    assert_eq!(p["enough"], true, "{p}");
    g.shot("sell-similar");
    // All of it on hover.
    let (x, y) = at_of(&mut g, "market.sell.similar");
    g.act(&format!("move {x} {y}"));
    g.shot("sell-similar-tip");
    // Price check: the graph over the form, which alone takes clicks.
    g.act("click ui:market.sell.price_check");
    let m = g.market();
    assert_eq!(m["open"], false, "the window stays shut");
    let p = m["prices"].clone();
    assert_eq!(p["open"], true, "{p}");
    assert_eq!(p["waiting"], false, "{p}");
    assert!(p["strips"].as_u64() >= Some(1), "{p}");
    let ids = g.ids();
    assert!(ids.iter().any(|i| i == "market.price.days(7)"));
    assert!(!ids.iter().any(|i| i == "market.sell.post"));
    // The newest point's tip.
    let pts = points(&mut g);
    let (x, y) = at_of(&mut g, pts.last().expect("the graph's points"));
    g.act(&format!("move {x} {y}"));
    g.shot("sell-price-check");
    g.act("click ui:market.price.days(90)");
    assert_eq!(g.market()["prices"]["days"], 90);
    // Esc shuts it; the form and its line stay.
    g.act("key ESC");
    let m = g.market();
    assert_eq!(m["prices"]["open"], false);
    assert_eq!(m["sell"]["item"].as_u64(), Some(item));
    assert!(m["prices"]["similar"].is_string(), "{m}");
    assert!(g.ids().iter().any(|i| i == "market.sell.post"));
    g.quit();
}
