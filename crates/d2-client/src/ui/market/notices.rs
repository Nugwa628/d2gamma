// D2RUST's own: the market's pushed notices and the Deliveries mark.
//! What happened to the player's listings and offers while they play
//! (contract 5, `MarketNotice`): a sale, an offer accepted, an offer back
//! because the listing went, a listing back at its expiry print a line in
//! the game's message area ("Your Grim Crown sold for 1 Lem + 2 Ko.");
//! every notice carries the Deliveries count, which the Market button
//! marks while anything waits, and refreshes the tab it is about when the
//! window shows it. Notices are not stored: an account away sees the
//! counts in the next `MarketState`.

use crate::market::trade::{Notice, NoticeWhat};

use super::details::price_text;
use super::{MarketTab, MarketUi};

/// The message area's line for notice `n`; only sales, accepted offers,
/// offers back and listings back print one (contract 5).
pub fn chat_line(ui: &MarketUi, n: &Notice) -> Option<String> {
    let price = n.price.as_ref().map(|p| price_text(ui, p));
    Some(match n.what {
        NoticeWhat::Sold => match price {
            Some(p) => format!("Your {} sold for {p}.", n.about),
            None => format!("Your {} sold.", n.about),
        },
        NoticeWhat::OfferAccepted => format!(
            "{} accepted your offer on {}. It waits in Deliveries.",
            who(n),
            n.about
        ),
        NoticeWhat::OfferReturned => format!(
            "Your offer on {} came back: the listing is gone. It waits in Deliveries.",
            n.about
        ),
        NoticeWhat::ListingExpired => {
            format!("Your {} listing expired. It waits in Deliveries.", n.about)
        }
        _ => return None,
    })
}

/// The other character, or "The seller" when the realm left it out.
fn who(n: &Notice) -> &str {
    if n.who.is_empty() {
        "The seller"
    } else {
        &n.who
    }
}

/// The count the Market button marks: Deliveries waiting, online with the
/// market open only.
pub fn mark(ui: &MarketUi) -> Option<u16> {
    if ui.availability() != super::Availability::Open {
        return None;
    }
    ui.state
        .as_ref()
        .map(|s| s.deliveries_waiting)
        .filter(|n| *n > 0)
}

/// A tab's label with its count: offers waiting on the player's answer,
/// deliveries waiting to be claimed.
pub fn tab_label(ui: &MarketUi, tab: MarketTab) -> String {
    let n = ui.state.as_ref().map_or(0, |s| match tab {
        MarketTab::Offers => s.live_offers_to_me,
        MarketTab::Deliveries => s.deliveries_waiting,
        _ => 0,
    });
    if n > 0 {
        format!("{} ({n})", tab.label())
    } else {
        tab.label().to_owned()
    }
}

impl MarketUi {
    /// A notice arrived: its chat line, the Deliveries count, and the
    /// shown lists asked for again.
    pub(super) fn notice(&mut self, n: Notice) {
        if let Some(line) = chat_line(self, &n) {
            self.chat.push(line);
        }
        if let Some(s) = self.state.as_mut() {
            s.deliveries_waiting = n.deliveries_waiting;
        }
        if !self.shown() {
            return;
        }
        match self.tab {
            MarketTab::Offers => self.open_offers(),
            MarketTab::Deliveries => self.open_deliveries(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{click, open};
    use super::super::window::Spot;
    use super::*;
    use crate::market::source::Price;
    use crate::market::trade::OfferId;

    fn notice(what: NoticeWhat, offer: Option<OfferId>) -> Notice {
        Notice {
            what,
            listing: 5,
            offer,
            about: "Grim Crown".into(),
            who: "Ilse".into(),
            price: Some(Price {
                gold: 0,
                stacks: vec![("r20".into(), 1), ("r24".into(), 2)],
                items: Vec::new(),
            }),
            deliveries_waiting: 3,
            at_ms: 0,
        }
    }

    #[test]
    fn notice_prints_chat_line() {
        let mut ui = MarketUi::new();
        ui.coin_names.insert("r20".into(), "Lem".into());
        ui.coin_names.insert("r24".into(), "Ko".into());
        assert_eq!(
            chat_line(&ui, &notice(NoticeWhat::Sold, None)).unwrap(),
            "Your Grim Crown sold for 1 Lem + 2 Ko."
        );
        // Sales, accepted offers, offers back and listings back print;
        // the rest only update the counts and the lists.
        for (code, prints) in [
            (1, true),
            (2, false),
            (3, false),
            (4, true),
            (5, false),
            (6, true),
            (7, true),
            (8, false),
        ] {
            let n = notice(NoticeWhat::of_code(code), Some(9));
            assert_eq!(chat_line(&ui, &n).is_some(), prints, "notice {code}");
        }
        // From the market: a sale of the player's listing prints its line.
        let (mut ui, mut src) = open();
        ui.chat.clear();
        src.sell_mine(9003, "Brakka");
        ui.pump(&mut src);
        assert_eq!(ui.chat, ["Your Korrin's Coil sold for 60,000 gold."]);
        // A new offer prints nothing but refreshes the shown Offers tab.
        click(&mut ui, &mut src, Spot::Tab(MarketTab::Offers));
        let mine = crate::market::trade::OffersWhich::OnMyListings;
        let before = ui.offers.page(mine).unwrap().rows.len();
        ui.chat.clear();
        src.offer_arrives(9001, "Ilse", Price::gold(150_000));
        ui.pump(&mut src);
        ui.pump(&mut src);
        assert!(ui.chat.is_empty());
        assert_eq!(ui.offers.page(mine).unwrap().rows.len(), before + 1);
    }

    #[test]
    fn stash_mark_follows_count() {
        // Single player: no mark.
        assert_eq!(mark(&MarketUi::new()), None);
        let (mut ui, mut src) = open();
        assert_eq!(mark(&ui), Some(5));
        assert_eq!(tab_label(&ui, MarketTab::Deliveries), "Deliveries (5)");
        assert_eq!(tab_label(&ui, MarketTab::Offers), "Offers (3)");
        // A notice carries the count.
        ui.apply(crate::market::source::MarketEvent::Notice(notice(
            NoticeWhat::OfferDeclined,
            Some(9),
        )));
        assert_eq!(mark(&ui), Some(3));
        // Everything claimed: the mark goes.
        click(&mut ui, &mut src, Spot::Tab(MarketTab::Deliveries));
        ui.at_stash = true;
        click(
            &mut ui,
            &mut src,
            Spot::Deliveries(super::super::deliveries::DeliverySpot::ClaimAll),
        );
        ui.pump(&mut src);
        assert_eq!(mark(&ui), None);
        assert_eq!(tab_label(&ui, MarketTab::Deliveries), "Deliveries");
        // A sale brings it back, with the window closed too.
        ui.close();
        src.sell_mine(9002, "Brakka");
        ui.pump(&mut src);
        assert_eq!(mark(&ui), Some(2));
        // The market closing takes it away.
        ui.set_online(false);
        assert_eq!(mark(&ui), None);
    }
}
