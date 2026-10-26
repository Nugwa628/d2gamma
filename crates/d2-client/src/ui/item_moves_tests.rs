//! The item-moving shortcuts' rules: which click is which shortcut, the
//! fit search, the messages and the tool tip lines.

use super::*;

const CTRL: Mods = Mods {
    ctrl: true,
    shift: false,
};
const CTRL_SHIFT: Mods = Mods {
    ctrl: true,
    shift: true,
};

fn open_stash() -> Open {
    Open {
        stash: true,
        ..Open::default()
    }
}

fn open_cube() -> Open {
    Open {
        cube: true,
        ..Open::default()
    }
}

fn open_trade(locked: bool) -> Open {
    Open {
        trade: Some(Trade { locked }),
        ..Open::default()
    }
}

fn texts(lines: &[TipLine]) -> Vec<String> {
    lines
        .iter()
        .map(|l| String::from_utf16_lossy(&l.text))
        .collect()
}

#[test]
fn ctrl_left_moves_between_the_inventory_and_the_open_window() {
    let all = ShortcutSet::ALL;
    let left = |from, open| shortcut_for(all, CTRL, Button::Left, from, open);
    assert_eq!(
        left(page::INVENTORY, open_stash()),
        Some((Shortcut::Stash, Action::Move { to: page::STASH }))
    );
    assert_eq!(
        left(page::STASH, open_stash()),
        Some((
            Shortcut::Stash,
            Action::Move {
                to: page::INVENTORY
            }
        ))
    );
    assert_eq!(
        left(page::INVENTORY, open_cube()),
        Some((Shortcut::Cube, Action::Move { to: page::CUBE }))
    );
    assert_eq!(
        left(page::CUBE, open_cube()),
        Some((
            Shortcut::Cube,
            Action::Move {
                to: page::INVENTORY
            }
        ))
    );
    assert_eq!(
        left(page::INVENTORY, open_trade(false)),
        Some((Shortcut::Trade, Action::Move { to: page::TRADE }))
    );
    assert_eq!(
        left(page::TRADE, open_trade(false)),
        Some((
            Shortcut::Trade,
            Action::Move {
                to: page::INVENTORY
            }
        ))
    );
    let store = Open {
        store: true,
        ..Open::default()
    };
    assert_eq!(
        left(page::INVENTORY, store),
        Some((Shortcut::Sell, Action::Sell))
    );
    // Nothing open: the plain Ctrl-click (which does nothing).
    assert_eq!(left(page::INVENTORY, Open::default()), None);
    // A stash item with the stash closed is not clicked at all.
    assert_eq!(left(page::STASH, open_cube()), None);
}

#[test]
fn plain_and_shift_clicks_are_never_shortcuts_here() {
    let all = ShortcutSet::ALL;
    for mods in [
        Mods::default(),
        Mods {
            ctrl: false,
            shift: true,
        },
    ] {
        for b in [Button::Left, Button::Right] {
            for from in [page::INVENTORY, page::STASH, page::CUBE] {
                assert_eq!(shortcut_for(all, mods, b, from, open_stash()), None);
            }
        }
    }
    // Ctrl+Shift+Left is not one either.
    assert_eq!(
        shortcut_for(all, CTRL_SHIFT, Button::Left, 0, open_stash()),
        None
    );
}

#[test]
fn ctrl_right_drops_and_ctrl_shift_right_puts_in_the_cube() {
    let all = ShortcutSet::ALL;
    assert_eq!(
        shortcut_for(all, CTRL, Button::Right, page::INVENTORY, Open::default()),
        Some((Shortcut::Drop, Action::Drop))
    );
    // Only inventory items drop.
    assert_eq!(
        shortcut_for(all, CTRL, Button::Right, page::STASH, open_stash()),
        None
    );
    for (from, open) in [
        (page::INVENTORY, Open::default()),
        (page::STASH, open_stash()),
    ] {
        assert_eq!(
            shortcut_for(all, CTRL_SHIFT, Button::Right, from, open),
            Some((Shortcut::IntoCube, Action::IntoCube))
        );
    }
    assert_eq!(
        shortcut_for(all, CTRL_SHIFT, Button::Right, page::CUBE, open_cube()),
        None
    );
}

#[test]
fn a_shortcut_switched_off_is_the_plain_click() {
    for s in Shortcut::ALL {
        let mut on = ShortcutSet::ALL;
        on.set(s, false);
        assert!(!on.has(s));
        let cases = [
            (CTRL, Button::Left, page::INVENTORY, open_stash()),
            (CTRL, Button::Left, page::STASH, open_stash()),
            (CTRL, Button::Left, page::INVENTORY, open_cube()),
            (CTRL, Button::Left, page::INVENTORY, open_trade(false)),
            (
                CTRL,
                Button::Left,
                page::INVENTORY,
                Open {
                    store: true,
                    ..Open::default()
                },
            ),
            (CTRL, Button::Right, page::INVENTORY, Open::default()),
            (CTRL_SHIFT, Button::Right, page::INVENTORY, Open::default()),
        ];
        for (m, b, from, open) in cases {
            if let Some((got, _)) = shortcut_for(on, m, b, from, open) {
                assert_ne!(got, s, "{s:?} is off");
            }
        }
    }
    // A mod game's switch-off wins over the settings.
    let on = ShortcutSet::ALL.without(ShortcutSet(Shortcut::Drop.bit()));
    assert_eq!(
        shortcut_for(on, CTRL, Button::Right, 0, Open::default()),
        None
    );
}

#[test]
fn every_shortcut_is_on_by_default_with_its_own_key() {
    let d = ShortcutSet::default();
    assert!(Shortcut::ALL.iter().all(|&s| d.has(s)));
    let mut keys: Vec<_> = Shortcut::ALL.iter().map(|s| s.key()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 10);
    assert_eq!(ShortcutSet::ALL.0, 0x3FF);
}

#[test]
fn the_fit_search_takes_the_first_free_spot_from_the_top_left() {
    // Empty 10 × 4 grid: the corner.
    assert_eq!(first_free_spot((10, 4), &[], (2, 3)), Some((0, 0)));
    // Column 0 full: the first free cell down column 1.
    let col0 = [(0, 0, 1, 4)];
    assert_eq!(first_free_spot((10, 4), &col0, (1, 1)), Some((1, 0)));
    // A 1 × 1 at (0, 0): the next free cell down the same column.
    assert_eq!(
        first_free_spot((10, 4), &[(0, 0, 1, 1)], (1, 1)),
        Some((0, 1))
    );
    // A two-handed 2 × 4 needs a whole free 2-wide column pair.
    let taken = [(0, 0, 1, 1), (3, 3, 1, 1)];
    assert_eq!(first_free_spot((10, 4), &taken, (2, 4)), Some((1, 0)));
    // Too big for the grid at all.
    assert_eq!(first_free_spot((3, 4), &[], (2, 5)), None);
    // A full grid.
    assert_eq!(first_free_spot((3, 4), &[(0, 0, 3, 4)], (1, 1)), None);
    // One free 1 × 1 cell left in a full 6 × 8 stash.
    let mut full: Vec<Rect> = (0..6)
        .flat_map(|x| (0..8).map(move |y| (x, y, 1, 1)))
        .collect();
    full.retain(|&(x, y, ..)| (x, y) != (5, 7));
    assert_eq!(first_free_spot((6, 8), &full, (1, 1)), Some((5, 7)));
    assert_eq!(first_free_spot((6, 8), &full, (1, 2)), None);
}

#[test]
fn a_move_is_the_lift_then_the_drag_s_own_drop_message() {
    let ids = |v: Vec<ClientIntent>| v.iter().map(|i| i.0.clone()).collect::<Vec<_>>();
    let to_stash = ids(move_intents(
        7,
        Dest::Page {
            page: page::STASH,
            x: 2,
            y: 5,
        },
    ));
    assert_eq!(to_stash.len(), 2);
    assert_eq!(to_stash[0], [0x19, 7, 0, 0, 0]);
    assert_eq!(
        to_stash[1],
        [0x18, 7, 0, 0, 0, 2, 0, 0, 0, 5, 0, 0, 0, 4, 0, 0, 0]
    );
    let dropped = ids(move_intents(7, Dest::Ground));
    assert_eq!(dropped[1], [0x17, 7, 0, 0, 0]);
    let cubed = ids(move_intents(7, Dest::Cube { cube: 9 }));
    assert_eq!(cubed[1], [0x2A, 7, 0, 0, 0, 9, 0, 0, 0]);
}

#[test]
fn the_tip_lists_the_shortcuts_that_work_now() {
    let all = ShortcutSet::ALL;
    let f = HintFacts {
        cube_in_inventory: true,
        ..HintFacts::default()
    };
    assert_eq!(
        texts(&hint_lines(all, page::INVENTORY, open_stash(), f)),
        [
            "Ctrl+Left: Move to stash",
            "Ctrl+Right: Drop",
            "Ctrl+Shift+Right: Put in cube"
        ]
    );
    assert!(hint_lines(all, 0, open_stash(), f)
        .iter()
        .all(|l| l.color == color::GREY));
    assert_eq!(
        texts(&hint_lines(all, page::STASH, open_stash(), f)),
        [
            "Ctrl+Left: Move to inventory",
            "Ctrl+Shift+Right: Put in cube"
        ]
    );
    assert_eq!(
        texts(&hint_lines(all, page::CUBE, open_cube(), f)),
        ["Ctrl+Left: Move to inventory"]
    );
    // No cube in the inventory, the inventory alone open.
    assert_eq!(
        texts(&hint_lines(all, 0, Open::default(), HintFacts::default())),
        ["Ctrl+Right: Drop"]
    );
    // The cube itself: not into the open cube, not into itself.
    let cube = HintFacts {
        is_cube: true,
        cube_in_inventory: true,
        ..HintFacts::default()
    };
    assert_eq!(
        texts(&hint_lines(all, 0, open_cube(), cube)),
        ["Ctrl+Right: Drop"]
    );
    // A locked trade and a trade's drop are left out.
    assert!(hint_lines(all, 0, open_trade(true), HintFacts::default()).is_empty());
    assert_eq!(
        texts(&hint_lines(all, 0, open_trade(false), HintFacts::default())),
        ["Ctrl+Left: Move to trade"]
    );
    // The vendor: sell only what it buys.
    let store = Open {
        store: true,
        ..Open::default()
    };
    let sellable = HintFacts {
        sellable: true,
        ..HintFacts::default()
    };
    assert_eq!(
        texts(&hint_lines(all, 0, store, sellable)),
        ["Ctrl+Left: Sell", "Ctrl+Right: Drop"]
    );
    assert_eq!(
        texts(&hint_lines(all, 0, store, HintFacts::default())),
        ["Ctrl+Right: Drop"]
    );
    // Everything off: no lines.
    assert!(hint_lines(ShortcutSet::NONE, 0, open_stash(), f).is_empty());
}

#[test]
fn refusals_say_why_in_a_few_words() {
    assert_eq!(
        Refusal::NoRoom(page::STASH).text(),
        "Not enough room in the stash"
    );
    assert_eq!(
        Refusal::NoRoom(page::INVENTORY).text(),
        "Not enough room in the inventory"
    );
    assert_eq!(Refusal::TradeLocked.text(), "The trade is locked");
}
