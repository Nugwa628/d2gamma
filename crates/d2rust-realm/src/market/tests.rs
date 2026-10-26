use std::path::PathBuf;
use std::sync::Arc;

use d2_accounts::AccountId;
use d2_economy::{Economy, EconomyConfig};
use d2_net::msg::account::CharSummary;
use d2_net::msg::market::{Ask, FromRealm, MarketQuery, OffReason, ToRealm};
use d2_net::msg::ErrorCode;
use tokio::sync::mpsc;

use super::{Market, MarketConfig, Who};
use crate::economy::Bank;
use crate::log::ServerLog;
use crate::session::{ControlOut, ControlTx};

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-realm-market")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn market(test: &str, on: bool) -> Arc<Market> {
    let config = EconomyConfig {
        market: on,
        ..EconomyConfig::default()
    };
    let economy = Economy::in_memory("d2rust", config.clone()).unwrap();
    let bank = Bank::start(economy).unwrap();
    let log = Arc::new(ServerLog::open_named(&temp(test), "realm.log").unwrap());
    Market::new(bank, "d2rust", config, None, log)
}

fn who(character: bool) -> (Who, mpsc::UnboundedReceiver<ControlOut>) {
    let (out, rx) = ControlTx::new(1);
    let character = character.then(|| CharSummary {
        name: "Seller".into(),
        class: 0,
        level: 1,
        status: d2_formats::d2s::status::EXPANSION,
        difficulties: 0,
        look: Vec::new(),
        last_played: 0,
        season: 0,
    });
    let who = Who {
        session: 1,
        account: AccountId(5),
        character,
        out,
    };
    (who, rx)
}

/// `ask` as request `req`; the one answer.
async fn ask(
    m: &Arc<Market>,
    who: &Who,
    rx: &mut mpsc::UnboundedReceiver<ControlOut>,
    req: u64,
    ask: Ask,
) -> FromRealm {
    m.clone().handle(who.clone(), ToRealm { req, ask }).await;
    match rx.recv().await {
        Some(ControlOut::Body(b)) => FromRealm::parse(&b).unwrap(),
        other => panic!("no answer: {other:?}"),
    }
}

fn code_of(answer: &FromRealm) -> ErrorCode {
    match answer {
        FromRealm::Done(d) => d.code,
        other => panic!("not a MarketDone: {other:?}"),
    }
}

#[tokio::test]
async fn market_off_answers_90() {
    let m = market("off", false);
    let (who, mut rx) = who(true);
    let search = Ask::Search(MarketQuery::default());
    assert_eq!(
        code_of(&ask(&m, &who, &mut rx, 1, search).await),
        ErrorCode::MARKET_OFF
    );
    let claim = Ask::Claim {
        all: true,
        deliveries: Vec::new(),
    };
    assert_eq!(
        code_of(&ask(&m, &who, &mut rx, 2, claim).await),
        ErrorCode::MARKET_OFF
    );
    // The hello still answers, with why.
    match ask(&m, &who, &mut rx, 3, Ask::Hello).await {
        FromRealm::State(s) => {
            assert!(!s.on);
            assert_eq!(s.off_reason, OffReason::Switch);
            assert_eq!(s.req, 3);
        }
        other => panic!("not a state: {other:?}"),
    }
}

#[tokio::test]
async fn no_character_answers_47() {
    let m = market("no-char", true);
    let (who, mut rx) = who(false);
    let search = Ask::Search(MarketQuery::default());
    assert_eq!(
        code_of(&ask(&m, &who, &mut rx, 1, search).await),
        ErrorCode::NO_CHAR_SELECTED
    );
    match ask(&m, &who, &mut rx, 2, Ask::Hello).await {
        FromRealm::State(s) => {
            assert!(!s.on);
            assert_eq!(s.off_reason, OffReason::NoCharacter);
        }
        other => panic!("not a state: {other:?}"),
    }
}

#[tokio::test]
async fn hello_carries_the_last_request() {
    let m = market("last-req", true);
    let (who, mut rx) = who(true);
    let claim = Ask::Claim {
        all: true,
        deliveries: Vec::new(),
    };
    let done = ask(&m, &who, &mut rx, 41, claim).await;
    assert_eq!(code_of(&done), ErrorCode(0));
    // The claim's own state push comes after its answer.
    let _state = rx.recv().await;
    match ask(&m, &who, &mut rx, 42, Ask::Hello).await {
        FromRealm::State(s) => {
            assert!(s.on);
            assert_eq!(s.off_reason, OffReason::On);
            assert_eq!(s.last_req, 41);
        }
        other => panic!("not a state: {other:?}"),
    }
}

#[tokio::test]
async fn searches_are_rate_limited_with_a_wait() {
    let m = market("rate", true);
    let (who, mut rx) = who(true);
    let per_min = EconomyConfig::default().market_searches_per_min as u64;
    for req in 1..=per_min {
        let a = ask(&m, &who, &mut rx, req, Ask::Search(MarketQuery::default())).await;
        assert!(matches!(a, FromRealm::ResultsBegin { .. }), "{req}: {a:?}");
        // The rest of the page: its end.
        while let Some(ControlOut::Body(b)) = rx.recv().await {
            if matches!(FromRealm::parse(&b).unwrap(), FromRealm::ListEnd { .. }) {
                break;
            }
        }
    }
    match ask(
        &m,
        &who,
        &mut rx,
        per_min + 1,
        Ask::Search(MarketQuery::default()),
    )
    .await
    {
        FromRealm::Done(d) => {
            assert_eq!(d.code, ErrorCode::RATE_LIMITED);
            assert!(d.wait_s > 0, "{d:?}");
        }
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn config_keys_parse() {
    let mut c = MarketConfig::default();
    assert!(!c.economy.market, "off by default");
    for (k, v) in [
        ("market", "on"),
        ("market_fee_pct", "5"),
        ("market_listing_fee", "100"),
        ("market_buyer_fee", "0"),
        ("market_max_listings", "40"),
        ("market_max_offers", "20"),
        ("market_offer_items", "4"),
        ("market_days", "7"),
        ("market_posts_per_min", "10"),
        ("market_searches_per_min", "60"),
        ("stash_items", "500"),
        ("stash_gold_max", "2500000"),
        ("stackables", "r01, gcv,"),
    ] {
        c.set(k, v).unwrap_or_else(|e| panic!("{k} = {v}: {e}"));
    }
    let e = &c.economy;
    assert!(e.market);
    assert_eq!(
        (
            e.market_fee_pct,
            e.market_listing_fee,
            e.market_max_listings,
            e.market_days
        ),
        (5, 100, 40, 7)
    );
    assert_eq!(
        (e.market_offer_items, e.stash_items, e.stash_gold_max),
        (4, 500, 2_500_000)
    );
    assert_eq!(c.stackables, ["r01", "gcv"]);
    for (k, v) in [
        ("market", "yes"),
        ("market_fee_pct", "101"),
        ("market_days", "0"),
        ("market_offer_items", "17"),
        ("stackables", "toolong"),
        ("market_nope", "1"),
    ] {
        assert!(c.set(k, v).is_err(), "{k} = {v}");
    }
    // On without game data: refused at start.
    assert!(c.economy_config(None).is_err());
    c.set("market", "off").unwrap();
    assert!(c.economy_config(None).is_ok());
}

/// With the market off the realm reads no item names, yet the stash still
/// keeps runes and gems as counts: its slots are the base game's.
#[test]
fn the_stash_has_its_slots_with_the_market_off() {
    let mut c = MarketConfig::default();
    let e = c.economy_config(None).unwrap();
    let codes: Vec<&str> = e.stackables.iter().map(|s| s.code.as_str()).collect();
    assert!(
        codes.contains(&"r01") && codes.contains(&"r33"),
        "{codes:?}"
    );
    assert!(
        codes.contains(&"gcv") && codes.contains(&"gpw"),
        "{codes:?}"
    );
    assert!(e.stackable("r01").is_some());
    // A configured list keeps only the slots it names.
    c.set("stackables", "r01, gcv").unwrap();
    let e = c.economy_config(None).unwrap();
    let codes: Vec<&str> = e.stackables.iter().map(|s| s.code.as_str()).collect();
    assert_eq!(codes, ["r01", "gcv"]);
}

#[test]
fn buyer_fee_must_be_zero() {
    let mut c = MarketConfig::default();
    let err = c.set("market_buyer_fee", "10").unwrap_err();
    assert!(err.contains("reserved"), "{err}");
    c.set("market_buyer_fee", "0").unwrap();
}
