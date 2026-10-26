//! The social store's rules (lobby contract sections 3-5, 8 and 10) on a
//! database in memory: one test per rule.

use d2_formats::d2s::status;
use d2_net::msg::account::CharSummary;
use d2_net::msg::ErrorCode;

use crate::db::Db;
use crate::social::*;
use crate::Denied;

const DAY: i64 = 24 * 3600 * 1000;
const T0: i64 = 1_800_000_000_000;

fn account(db: &Db, name: &str) -> i64 {
    db.insert_account(name, &name.to_ascii_lowercase(), "h", 1)
        .unwrap()
        .unwrap()
}

fn character(db: &mut Db, account: i64, name: &str, last_played: u64) {
    let s = CharSummary {
        name: name.into(),
        class: 1,
        level: 10,
        status: status::EXPANSION,
        difficulties: 1,
        look: vec![0xFF; 32],
        last_played,
        season: 0,
    };
    assert!(db.insert_character(account, &s, 10, 0).unwrap());
}

fn refused<T: std::fmt::Debug>(r: SocialResult<T>) -> Refusal {
    match r {
        Err(SocialError::Refused(why)) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// A database with accounts `A0`..`A{n-1}`.
fn with_accounts(n: usize) -> (Db, Vec<i64>) {
    let db = Db::in_memory().unwrap();
    let ids = (0..n).map(|i| account(&db, &format!("A{i}"))).collect();
    (db, ids)
}

// --- Players ---

#[test]
fn find_player_by_character_or_account() {
    let (mut db, a) = with_accounts(2);
    character(&mut db, a[1], "Kashya", 5);
    let p = db.find_player("KASHYA").unwrap().unwrap();
    assert_eq!(
        (p.account, p.account_name.as_str(), p.char_name.as_deref()),
        (a[1], "A1", Some("Kashya"))
    );
    let p = db.find_player("*a0").unwrap().unwrap();
    assert_eq!((p.account, p.char_name), (a[0], None));
    assert_eq!(db.find_player("Akara").unwrap(), None);
    assert_eq!(db.find_player("*nobody").unwrap(), None);
    // A deleted character names nobody.
    db.mark_deleted(a[1], "kashya", 9).unwrap();
    assert_eq!(db.find_player("Kashya").unwrap(), None);
}

#[test]
fn last_character_is_the_last_played() {
    let (mut db, a) = with_accounts(1);
    assert_eq!(db.last_character(a[0]).unwrap(), None);
    character(&mut db, a[0], "Kashya", 50);
    character(&mut db, a[0], "Akara", 70);
    character(&mut db, a[0], "Charsi", 60);
    assert_eq!(db.last_character(a[0]).unwrap().as_deref(), Some("Akara"));
    db.mark_deleted(a[0], "akara", 80).unwrap();
    assert_eq!(db.last_character(a[0]).unwrap().as_deref(), Some("Charsi"));
}

#[test]
fn refusals_carry_the_contract_codes() {
    let codes = [
        (Refusal::NotInGuild, 121),
        (Refusal::GuildRank, 122),
        (Refusal::GuildTagTaken, 123),
        (Refusal::BadGuildName("x"), 124),
        (Refusal::AlreadyInGuild, 125),
        (Refusal::GuildFull, 126),
        (Refusal::UnknownPlayer, 127),
        (Refusal::InboxFull, 128),
        (Refusal::WhisperBlocked, 129),
        (Refusal::FriendLimit, 130),
        (Refusal::BadText, 134),
        (Refusal::InviteGone, 135),
    ];
    for (r, code) in codes {
        assert_eq!(r.code(), ErrorCode(code), "{r:?}");
        let d: Denied = r.into();
        assert_eq!((d.code, d.wait_s), (ErrorCode(code), 0));
        assert!(!d.text.is_empty());
    }
    let d: Denied = Refusal::BadGuildName("A guild tag must be 2 to 4 characters long.").into();
    assert_eq!(d.text, "A guild tag must be 2 to 4 characters long.");
}

// --- Guilds ---

#[test]
fn guild_tag_unique_any_case() {
    let (db, a) = with_accounts(2);
    let g = db.guilds().create(a[0], "NUG", "Nugwa", T0).unwrap();
    assert_eq!(
        refused(db.guilds().create(a[1], "nug", "Other Guild", T0)),
        Refusal::GuildTagTaken
    );
    let found = db.guilds().by_tag("nUG").unwrap().unwrap();
    assert_eq!(
        (found.id, found.tag.as_str(), found.members, found.leader),
        (g, "NUG", 1, a[0])
    );
    assert!(db.guilds().create(a[1], "NUG2", "Other Guild", T0).is_ok());
}

#[test]
fn guild_tag_and_name_rules() {
    let (db, a) = with_accounts(1);
    for tag in ["W", "WGAME", "W-G", "W G", "ÄB"] {
        assert!(
            matches!(
                refused(db.guilds().create(a[0], tag, "Nugwa", T0)),
                Refusal::BadGuildName(_)
            ),
            "{tag:?}"
        );
    }
    let long = "A".repeat(25);
    for name in [
        "Ab",
        long.as_str(),
        " Lead",
        "End ",
        "Two  Spaces",
        "Bad!",
        "Tab\tName",
    ] {
        assert!(
            matches!(
                refused(db.guilds().create(a[0], "NUG", name, T0)),
                Refusal::BadGuildName(_)
            ),
            "{name:?}"
        );
    }
    let max = "B".repeat(24);
    assert!(check_guild_name(&max).is_ok());
    assert!(check_guild_tag("a1").is_ok());
    assert!(db.guilds().create(a[0], "NUG", "Nugwa 2", T0).is_ok());
}

#[test]
fn one_guild_per_account() {
    let (db, a) = with_accounts(3);
    let g0 = db.guilds().create(a[0], "NUG", "Nugwa", T0).unwrap();
    db.guilds().create(a[1], "PD", "Path Divers", T0).unwrap();
    assert_eq!(
        refused(db.guilds().create(a[0], "XX", "Second Guild", T0)),
        Refusal::AlreadyInGuild
    );
    assert_eq!(
        refused(db.guilds().invite(a[0], a[1], T0, GUILD_MAX_MEMBERS)),
        Refusal::AlreadyInGuild
    );
    // An invite made before the invitee joined another guild.
    db.guilds()
        .invite(a[0], a[2], T0, GUILD_MAX_MEMBERS)
        .unwrap();
    db.guilds().create(a[2], "ZZ", "Third Guild", T0).unwrap();
    assert_eq!(
        db.guilds().invites(a[2], T0).unwrap(),
        [],
        "joining drops the invites"
    );
    assert_eq!(
        refused(db.guilds().answer(a[2], g0, true, T0, GUILD_MAX_MEMBERS)),
        Refusal::InviteGone
    );
    assert_eq!(
        db.guilds().membership(a[0]).unwrap(),
        Some((g0, Rank::Leader))
    );
}

#[test]
fn invites_need_a_rank_and_expire() {
    let (db, a) = with_accounts(5);
    let g = db.guilds().create(a[0], "NUG", "Nugwa", T0).unwrap();
    let max = GUILD_MAX_MEMBERS;
    assert_eq!(
        refused(db.guilds().invite(a[1], a[2], T0, max)),
        Refusal::NotInGuild
    );
    db.guilds().invite(a[0], a[1], T0, max).unwrap();
    db.guilds().answer(a[1], g, true, T0, max).unwrap();
    // A member can't invite; an officer can.
    assert_eq!(
        refused(db.guilds().invite(a[1], a[2], T0, max)),
        Refusal::GuildRank
    );
    db.guilds().promote(a[0], a[1]).unwrap();
    assert_eq!(db.guilds().invite(a[1], a[2], T0, max).unwrap(), g);
    assert_eq!(
        refused(db.guilds().invite(a[0], 999, T0, max)),
        Refusal::UnknownPlayer
    );
    let invites = db.guilds().invites(a[2], T0 + 1).unwrap();
    assert_eq!(invites.len(), 1);
    assert_eq!((invites[0].id, invites[0].members), (g, 2));
    // Seven days later it is gone.
    let later = T0 + INVITE_DAYS * DAY;
    assert_eq!(db.guilds().invites(a[2], later).unwrap(), []);
    assert_eq!(
        refused(db.guilds().answer(a[2], g, true, later, max)),
        Refusal::InviteGone
    );
    // A repeated invite starts its days again.
    db.guilds().invite(a[0], a[2], later - 1, max).unwrap();
    assert!(db.guilds().answer(a[2], g, true, later, max).is_ok());
    // Declining drops the invite.
    db.guilds().invite(a[0], a[3], T0, max).unwrap();
    db.guilds().answer(a[3], g, false, T0, max).unwrap();
    assert_eq!(
        refused(db.guilds().answer(a[3], g, true, T0, max)),
        Refusal::InviteGone
    );
    // The sweep deletes the old ones.
    db.guilds().invite(a[0], a[4], T0, max).unwrap();
    assert_eq!(
        db.guilds()
            .sweep_invites(T0 + INVITE_DAYS * DAY - 1)
            .unwrap(),
        0
    );
    assert_eq!(
        db.guilds().sweep_invites(T0 + INVITE_DAYS * DAY).unwrap(),
        1
    );
    // No invite at all.
    assert_eq!(
        refused(db.guilds().answer(a[4], 12345, true, T0, max)),
        Refusal::InviteGone
    );
}

#[test]
fn accepting_drops_the_other_invites() {
    let (db, a) = with_accounts(3);
    let g0 = db.guilds().create(a[0], "NUG", "Nugwa", T0).unwrap();
    let g1 = db.guilds().create(a[1], "PD", "Path Divers", T0).unwrap();
    db.guilds()
        .invite(a[0], a[2], T0, GUILD_MAX_MEMBERS)
        .unwrap();
    db.guilds()
        .invite(a[1], a[2], T0 + 1, GUILD_MAX_MEMBERS)
        .unwrap();
    let ids: Vec<i64> = db
        .guilds()
        .invites(a[2], T0 + 2)
        .unwrap()
        .iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(ids, [g1, g0], "newest invite first");
    db.guilds()
        .answer(a[2], g1, true, T0 + 2, GUILD_MAX_MEMBERS)
        .unwrap();
    assert_eq!(db.guilds().invites(a[2], T0 + 2).unwrap(), []);
    assert_eq!(
        db.guilds().membership(a[2]).unwrap(),
        Some((g1, Rank::Member))
    );
}

#[test]
fn leader_cant_leave_with_members() {
    let (db, a) = with_accounts(2);
    let g = db.guilds().create(a[0], "NUG", "Nugwa", T0).unwrap();
    db.guilds()
        .invite(a[0], a[1], T0, GUILD_MAX_MEMBERS)
        .unwrap();
    db.guilds()
        .answer(a[1], g, true, T0, GUILD_MAX_MEMBERS)
        .unwrap();
    assert_eq!(refused(db.guilds().leave(a[0])), Refusal::GuildRank);
    assert_eq!(
        db.guilds().leave(a[1]).unwrap(),
        Left {
            guild: g,
            disbanded: false
        }
    );
    assert_eq!(refused(db.guilds().leave(a[1])), Refusal::NotInGuild);
    // Handing over first lets the old leader leave.
    db.guilds()
        .invite(a[0], a[1], T0, GUILD_MAX_MEMBERS)
        .unwrap();
    db.guilds()
        .answer(a[1], g, true, T0, GUILD_MAX_MEMBERS)
        .unwrap();
    db.guilds().make_leader(a[0], a[1]).unwrap();
    assert!(!db.guilds().leave(a[0]).unwrap().disbanded);
    assert_eq!(db.guilds().get(g).unwrap().unwrap().leader, a[1]);
}

#[test]
fn last_member_leaving_disbands() {
    let (db, a) = with_accounts(3);
    let g = db.guilds().create(a[0], "NUG", "Nugwa", T0).unwrap();
    db.guilds()
        .invite(a[0], a[2], T0, GUILD_MAX_MEMBERS)
        .unwrap();
    db.guilds().set_motd(a[0], "Hi").unwrap();
    assert_eq!(
        db.guilds().leave(a[0]).unwrap(),
        Left {
            guild: g,
            disbanded: true
        }
    );
    assert_eq!(db.guilds().get(g).unwrap(), None);
    assert_eq!(db.guilds().invites(a[2], T0).unwrap(), []);
    // The tag is free at once.
    assert!(db.guilds().create(a[1], "nug", "New Nugwa", T0).is_ok());
}

#[test]
fn guild_full() {
    let (db, a) = with_accounts(4);
    let g = db.guilds().create(a[0], "NUG", "Nugwa", T0).unwrap();
    db.guilds().invite(a[0], a[1], T0, 2).unwrap();
    db.guilds().invite(a[0], a[2], T0, 2).unwrap();
    db.guilds().answer(a[1], g, true, T0, 2).unwrap();
    assert_eq!(
        refused(db.guilds().answer(a[2], g, true, T0, 2)),
        Refusal::GuildFull
    );
    assert_eq!(
        refused(db.guilds().invite(a[0], a[3], T0, 2)),
        Refusal::GuildFull
    );
    assert_eq!(db.guilds().get(g).unwrap().unwrap().members, 2);
}

/// A guild of a leader (0), two officers (1, 2) and two members (3, 4);
/// account 5 is outside.
fn full_guild() -> (Db, Vec<i64>, i64) {
    let (db, a) = with_accounts(6);
    let g = db.guilds().create(a[0], "NUG", "Nugwa", T0).unwrap();
    for (i, &m) in a[1..5].iter().enumerate() {
        db.guilds().invite(a[0], m, T0, GUILD_MAX_MEMBERS).unwrap();
        db.guilds()
            .answer(
                m,
                g,
                true,
                T0 + i64::try_from(i).unwrap(),
                GUILD_MAX_MEMBERS,
            )
            .unwrap();
    }
    db.guilds().promote(a[0], a[1]).unwrap();
    db.guilds().promote(a[0], a[2]).unwrap();
    (db, a, g)
}

#[test]
fn kick_rules() {
    let (db, a, g) = full_guild();
    assert_eq!(
        refused(db.guilds().kick(a[1], a[0])),
        Refusal::GuildRank,
        "an officer can't kick the leader"
    );
    assert_eq!(
        refused(db.guilds().kick(a[1], a[2])),
        Refusal::GuildRank,
        "nor another officer"
    );
    assert_eq!(
        refused(db.guilds().kick(a[3], a[4])),
        Refusal::GuildRank,
        "a member can't kick"
    );
    assert_eq!(
        refused(db.guilds().kick(a[0], a[0])),
        Refusal::GuildRank,
        "nobody kicks themselves"
    );
    assert_eq!(
        refused(db.guilds().kick(a[0], a[5])),
        Refusal::UnknownPlayer
    );
    assert_eq!(refused(db.guilds().kick(a[5], a[3])), Refusal::NotInGuild);
    assert_eq!(db.guilds().kick(a[1], a[3]).unwrap(), g);
    assert_eq!(
        db.guilds().kick(a[0], a[2]).unwrap(),
        g,
        "the leader kicks anyone"
    );
    assert_eq!(db.guilds().membership(a[3]).unwrap(), None);
    assert_eq!(db.guilds().get(g).unwrap().unwrap().members, 3);
}

#[test]
fn ranks_promote_demote_hand_over() {
    let (db, a, g) = full_guild();
    assert_eq!(
        refused(db.guilds().promote(a[0], a[1])),
        Refusal::GuildRank,
        "already an officer"
    );
    assert_eq!(
        refused(db.guilds().promote(a[1], a[3])),
        Refusal::GuildRank,
        "only the leader promotes"
    );
    assert_eq!(
        refused(db.guilds().demote(a[0], a[3])),
        Refusal::GuildRank,
        "already a member"
    );
    db.guilds().demote(a[0], a[2]).unwrap();
    assert_eq!(
        db.guilds().membership(a[2]).unwrap(),
        Some((g, Rank::Member))
    );
    assert_eq!(
        refused(db.guilds().make_leader(a[1], a[3])),
        Refusal::GuildRank
    );
    db.guilds().make_leader(a[0], a[3]).unwrap();
    assert_eq!(
        db.guilds().membership(a[0]).unwrap(),
        Some((g, Rank::Officer))
    );
    assert_eq!(
        db.guilds().membership(a[3]).unwrap(),
        Some((g, Rank::Leader))
    );
    let ranks: Vec<(i64, Rank)> = db
        .guilds()
        .members(g)
        .unwrap()
        .iter()
        .map(|m| (m.account, m.rank))
        .collect();
    assert_eq!(
        ranks,
        [
            (a[3], Rank::Leader),
            (a[0], Rank::Officer),
            (a[1], Rank::Officer),
            (a[2], Rank::Member),
            (a[4], Rank::Member)
        ],
        "leader, officers, members; each by join time"
    );
}

#[test]
fn disband_needs_the_leader_and_the_tag() {
    let (db, a, g) = full_guild();
    db.guilds()
        .invite(a[0], a[5], T0, GUILD_MAX_MEMBERS)
        .unwrap();
    assert_eq!(
        refused(db.guilds().disband(a[1], "NUG")),
        Refusal::GuildRank
    );
    assert!(matches!(
        refused(db.guilds().disband(a[0], "XX")),
        Refusal::BadGuildName(_)
    ));
    let (gone, members) = db.guilds().disband(a[0], "nug").unwrap();
    assert_eq!((gone, members), (g, a[..5].to_vec()));
    assert_eq!(db.guilds().get(g).unwrap(), None);
    assert_eq!(db.guilds().membership(a[3]).unwrap(), None);
    assert_eq!(db.guilds().invites(a[5], T0).unwrap(), []);
    assert!(db.guilds().create(a[1], "NUG", "Nugwa Again", T0).is_ok());
}

#[test]
fn motd_rules() {
    let (db, a, g) = full_guild();
    assert_eq!(db.guilds().set_motd(a[1], "Rush at 8").unwrap(), g);
    assert_eq!(db.guilds().get(g).unwrap().unwrap().motd, "Rush at 8");
    assert_eq!(refused(db.guilds().set_motd(a[3], "x")), Refusal::GuildRank);
    assert_eq!(
        refused(db.guilds().set_motd(a[5], "x")),
        Refusal::NotInGuild
    );
    assert_eq!(
        refused(db.guilds().set_motd(a[0], &"m".repeat(MOTD_MAX + 1))),
        Refusal::BadText
    );
    assert_eq!(
        refused(db.guilds().set_motd(a[0], "line\nbreak")),
        Refusal::BadText
    );
    db.guilds().set_motd(a[0], &"m".repeat(MOTD_MAX)).unwrap();
    db.guilds().set_motd(a[0], "").unwrap();
}

#[test]
fn members_show_their_last_character_and_last_seen() {
    let (mut db, a) = with_accounts(2);
    character(&mut db, a[0], "Kashya", 5);
    let g = db.guilds().create(a[0], "NUG", "Nugwa", T0).unwrap();
    db.guilds()
        .invite(a[0], a[1], T0, GUILD_MAX_MEMBERS)
        .unwrap();
    db.guilds()
        .answer(a[1], g, true, T0 + 5, GUILD_MAX_MEMBERS)
        .unwrap();
    db.friends().set_last_seen(a[1], 1_700_000_000).unwrap();
    let m = db.guilds().members(g).unwrap();
    assert_eq!(
        (m[0].last_char.as_deref(), m[0].last_seen, m[0].joined_ms),
        (Some("Kashya"), None, T0)
    );
    assert_eq!(
        (
            m[1].account_name.as_str(),
            m[1].last_char.as_deref(),
            m[1].last_seen
        ),
        ("A1", None, Some(1_700_000_000))
    );
    assert_eq!(
        db.guilds().get(g).unwrap().unwrap().leader_char.as_deref(),
        Some("Kashya")
    );
}

// --- Friends and ignores ---

#[test]
fn friends_are_one_sided_until_mutual() {
    let (db, a) = with_accounts(3);
    let f = db.friends();
    assert!(f.add_friend(a[0], a[1], T0, FRIENDS_MAX).unwrap());
    assert!(
        !f.add_friend(a[0], a[1], T0 + 1, FRIENDS_MAX).unwrap(),
        "a repeat"
    );
    let rows = f.list_friends(a[0]).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].account, rows[0].mutual, rows[0].added_ms),
        (a[1], false, T0)
    );
    assert!(!f.is_mutual(a[0], a[1]).unwrap());
    assert_eq!(f.list_friends(a[1]).unwrap(), [], "a1 added nobody");
    f.add_friend(a[1], a[0], T0, FRIENDS_MAX).unwrap();
    assert!(f.is_mutual(a[1], a[0]).unwrap());
    assert!(f.list_friends(a[0]).unwrap()[0].mutual);
    f.add_friend(a[2], a[0], T0, FRIENDS_MAX).unwrap();
    assert_eq!(f.followers(a[0]).unwrap(), [a[1], a[2]]);
    assert!(f.remove_friend(a[0], a[1]).unwrap());
    assert!(!f.remove_friend(a[0], a[1]).unwrap());
    assert!(!f.is_friend(a[0], a[1]).unwrap());
    assert!(!f.list_friends(a[1]).unwrap()[0].mutual);
    assert_eq!(
        refused(f.add_friend(a[0], a[0], T0, FRIENDS_MAX)),
        Refusal::UnknownPlayer
    );
    assert_eq!(
        refused(f.add_friend(a[0], 999, T0, FRIENDS_MAX)),
        Refusal::UnknownPlayer
    );
}

#[test]
fn friends_max() {
    let (db, a) = with_accounts(4);
    let f = db.friends();
    f.add_friend(a[0], a[1], T0, 2).unwrap();
    f.add_friend(a[0], a[2], T0, 2).unwrap();
    assert_eq!(
        refused(f.add_friend(a[0], a[3], T0, 2)),
        Refusal::FriendLimit
    );
    // A repeat of a friend on a full list is no error.
    assert!(!f.add_friend(a[0], a[1], T0, 2).unwrap());
    // The ignore list has its own room.
    f.ignore(a[0], a[3], T0, 1).unwrap();
    assert_eq!(refused(f.ignore(a[0], a[2], T0, 1)), Refusal::FriendLimit);
    f.remove_friend(a[0], a[1]).unwrap();
    assert!(f.add_friend(a[0], a[3], T0, 2).unwrap());
}

#[test]
fn ignores_and_their_lists() {
    let (db, a) = with_accounts(3);
    let f = db.friends();
    assert!(f.ignore(a[0], a[2], T0, IGNORE_MAX).unwrap());
    assert!(f.ignore(a[0], a[1], T0, IGNORE_MAX).unwrap());
    assert!(!f.ignore(a[0], a[1], T0, IGNORE_MAX).unwrap());
    assert!(f.ignores(a[0], a[1]).unwrap());
    assert!(!f.ignores(a[1], a[0]).unwrap());
    assert_eq!(f.ignored_ids(a[0]).unwrap(), [a[1], a[2]]);
    let names: Vec<String> = f
        .list_ignored(a[0])
        .unwrap()
        .into_iter()
        .map(|r| r.account_name)
        .collect();
    assert_eq!(names, ["A1", "A2"]);
    assert_eq!(
        refused(f.ignore(a[0], a[0], T0, IGNORE_MAX)),
        Refusal::UnknownPlayer
    );
    assert!(f.unignore(a[0], a[1]).unwrap());
    assert!(!f.unignore(a[0], a[1]).unwrap());
    assert_eq!(f.ignored_ids(a[0]).unwrap(), [a[2]]);
}

#[test]
fn last_seen_round_trip() {
    let (mut db, a) = with_accounts(2);
    character(&mut db, a[1], "Akara", 3);
    let f = db.friends();
    assert_eq!(f.last_seen(a[1]).unwrap(), None);
    f.set_last_seen(a[1], 1_700_000_123).unwrap();
    assert_eq!(f.last_seen(a[1]).unwrap(), Some(1_700_000_123));
    f.add_friend(a[0], a[1], T0, FRIENDS_MAX).unwrap();
    let row = &f.list_friends(a[0]).unwrap()[0];
    assert_eq!(
        (row.last_seen, row.last_char.as_deref()),
        (Some(1_700_000_123), Some("Akara"))
    );
    assert_eq!(f.last_seen(999).unwrap(), None);
}

// --- Privacy ---

#[test]
fn privacy_round_trip() {
    let (db, a) = with_accounts(1);
    assert_eq!(db.prefs().get(a[0]).unwrap(), Privacy::default());
    let p = Privacy {
        whispers: Whispers::Nobody,
        show_where: ShowWhere::Nobody,
    };
    db.prefs().set(a[0], p).unwrap();
    assert_eq!(db.prefs().get(a[0]).unwrap(), p);
    assert_eq!(refused(db.prefs().set(999, p)), Refusal::UnknownPlayer);
    assert_eq!(Whispers::from_u8(3), None);
    assert_eq!(ShowWhere::from_u8(2), None);
    assert_eq!(Whispers::from_u8(1).unwrap().as_u8(), 1);
}

#[test]
fn whispers_blocked_by_ignore_and_privacy() {
    let (db, a) = with_accounts(4);
    let dms = db.dms();
    let send =
        |from: i64, to: i64| dms.send(from, to, "F", "T", "hi", T0, true, DmLimits::default());
    // Ignore blocks, whatever the privacy.
    db.friends().ignore(a[1], a[0], T0, IGNORE_MAX).unwrap();
    assert_eq!(refused(send(a[0], a[1])), Refusal::WhisperBlocked);
    db.friends().unignore(a[1], a[0]).unwrap();
    assert!(send(a[0], a[1]).is_ok());
    // Friends and guild only.
    let p = |whispers| Privacy {
        whispers,
        show_where: ShowWhere::MutualFriends,
    };
    db.prefs().set(a[2], p(Whispers::FriendsAndGuild)).unwrap();
    assert_eq!(refused(send(a[0], a[2])), Refusal::WhisperBlocked);
    db.friends()
        .add_friend(a[0], a[2], T0, FRIENDS_MAX)
        .unwrap();
    assert_eq!(
        refused(send(a[0], a[2])),
        Refusal::WhisperBlocked,
        "a2 must have added a0"
    );
    db.friends()
        .add_friend(a[2], a[0], T0, FRIENDS_MAX)
        .unwrap();
    assert!(send(a[0], a[2]).is_ok());
    let g = db.guilds().create(a[2], "NUG", "Nugwa", T0).unwrap();
    db.guilds()
        .invite(a[2], a[3], T0, GUILD_MAX_MEMBERS)
        .unwrap();
    assert_eq!(refused(send(a[3], a[2])), Refusal::WhisperBlocked);
    db.guilds()
        .answer(a[3], g, true, T0, GUILD_MAX_MEMBERS)
        .unwrap();
    assert!(send(a[3], a[2]).is_ok(), "a guildmate");
    // Nobody, but a reply to a2's own DM passes.
    db.prefs().set(a[2], p(Whispers::Nobody)).unwrap();
    assert_eq!(refused(send(a[1], a[2])), Refusal::WhisperBlocked);
    send(a[2], a[1]).unwrap();
    assert!(send(a[1], a[2]).is_ok());
    assert!(dms.may_whisper(a[1], a[2]).unwrap());
    assert!(
        !dms.may_whisper(a[0], a[2]).unwrap(),
        "a friend, but a2 never wrote to a0"
    );
}

// --- DMs ---

#[test]
fn dm_text_rules() {
    let (db, a) = with_accounts(2);
    let send = |text: &str| {
        db.dms()
            .send(a[0], a[1], "F", "T", text, T0, true, DmLimits::default())
    };
    assert_eq!(refused(send("")), Refusal::BadText);
    assert_eq!(refused(send(&"x".repeat(201))), Refusal::BadText);
    assert_eq!(refused(send("bell\u{7}")), Refusal::BadText);
    assert!(send(&"x".repeat(200)).is_ok());
    assert_eq!(
        refused(
            db.dms()
                .send(a[0], a[0], "F", "F", "me", T0, true, DmLimits::default())
        ),
        Refusal::UnknownPlayer
    );
    assert_eq!(
        refused(
            db.dms()
                .send(a[0], 999, "F", "T", "hi", T0, false, DmLimits::default())
        ),
        Refusal::UnknownPlayer
    );
}

#[test]
fn dm_offline_lines_wait_and_deliver() {
    let (db, a) = with_accounts(2);
    let dms = db.dms();
    let l = DmLimits::default();
    let s0 = dms
        .send(a[0], a[1], "Kashya", "Akara", "one", T0, false, l)
        .unwrap();
    let s1 = dms
        .send(a[0], a[1], "Kashya", "Akara", "two", T0 + 1, false, l)
        .unwrap();
    assert!(s0.stored && s1.stored && s1.id > s0.id);
    let online = dms
        .send(a[1], a[0], "Akara", "Kashya", "back", T0 + 2, true, l)
        .unwrap();
    assert!(!online.stored);
    let waiting = dms.undelivered(a[1]).unwrap();
    assert_eq!(
        waiting.iter().map(|d| d.text.as_str()).collect::<Vec<_>>(),
        ["one", "two"]
    );
    assert_eq!(
        (
            waiting[0].from_char.as_str(),
            waiting[0].to_char.as_str(),
            waiting[0].peer_of(a[1])
        ),
        ("Kashya", "Akara", a[0])
    );
    assert_eq!(dms.undelivered(a[0]).unwrap(), []);
    assert_eq!(dms.mark_delivered(a[1], s0.id).unwrap(), 1);
    assert_eq!(dms.undelivered(a[1]).unwrap().len(), 1);
    assert_eq!(dms.mark_delivered(a[1], i64::MAX).unwrap(), 1);
    assert_eq!(dms.undelivered(a[1]).unwrap(), []);
}

#[test]
fn dm_waiting_caps_per_recipient_and_sender() {
    let (db, a) = with_accounts(4);
    let dms = db.dms();
    let l = DmLimits {
        waiting_max: 5,
        from_one_max: 3,
        keep_days: 30,
    };
    for i in 0..3 {
        dms.send(a[0], a[3], "F", "T", &format!("a{i}"), T0, false, l)
            .unwrap();
    }
    assert_eq!(
        refused(dms.send(a[0], a[3], "F", "T", "a3", T0, false, l)),
        Refusal::InboxFull
    );
    // A line for an online peer doesn't wait, so no cap.
    assert!(dms.send(a[0], a[3], "F", "T", "live", T0, true, l).is_ok());
    dms.send(a[1], a[3], "F", "T", "b0", T0, false, l).unwrap();
    dms.send(a[1], a[3], "F", "T", "b1", T0, false, l).unwrap();
    assert_eq!(
        refused(dms.send(a[2], a[3], "F", "T", "c0", T0, false, l)),
        Refusal::InboxFull,
        "five waiting for a3"
    );
    // Delivered lines free the box.
    dms.mark_delivered(a[3], i64::MAX).unwrap();
    assert!(dms.send(a[2], a[3], "F", "T", "c0", T0, false, l).is_ok());
    assert!(dms.send(a[0], a[3], "F", "T", "a3", T0, false, l).is_ok());
    // The defaults are the contract's: 200 per recipient, 20 from one.
    assert_eq!(
        DmLimits::default(),
        DmLimits {
            waiting_max: 200,
            from_one_max: 20,
            keep_days: 30
        }
    );
}

#[test]
fn dm_keep_days() {
    let (db, a) = with_accounts(2);
    let dms = db.dms();
    let l = DmLimits::default();
    dms.send(a[0], a[1], "F", "T", "old", T0, true, l).unwrap();
    dms.send(a[1], a[0], "T", "F", "new", T0 + DAY, false, l)
        .unwrap();
    assert_eq!(dms.sweep(T0 + 30 * DAY - 1, l.keep_days).unwrap(), 0);
    assert_eq!(dms.sweep(T0 + 30 * DAY, l.keep_days).unwrap(), 1);
    let left = dms.history(a[0], a[1], 0, 50).unwrap();
    assert_eq!(
        left.iter().map(|d| d.text.as_str()).collect::<Vec<_>>(),
        ["new"]
    );
    assert_eq!(
        dms.sweep(T0 + 31 * DAY, l.keep_days).unwrap(),
        1,
        "undelivered lines go too"
    );
    assert!(!dms.has_conversation(a[0], a[1]).unwrap());
}

#[test]
fn dm_history_pages_without_overlap() {
    let (db, a) = with_accounts(3);
    let dms = db.dms();
    let l = DmLimits::default();
    let mut ids = Vec::new();
    for i in 0..7 {
        let (from, to) = if i % 2 == 0 {
            (a[0], a[1])
        } else {
            (a[1], a[0])
        };
        ids.push(
            dms.send(from, to, "F", "T", &format!("{i}"), T0 + i, true, l)
                .unwrap()
                .id,
        );
        // Another conversation in between stays out.
        dms.send(a[2], a[0], "X", "F", "other", T0 + i, true, l)
            .unwrap();
    }
    let mut pages = Vec::new();
    let mut before = 0;
    loop {
        let page = dms.history(a[1], a[0], before, 3).unwrap();
        if page.is_empty() {
            break;
        }
        assert!(page.windows(2).all(|w| w[0].id < w[1].id), "oldest first");
        before = page[0].id;
        pages.push(page.iter().map(|d| d.id).collect::<Vec<_>>());
    }
    assert_eq!(
        pages,
        [ids[4..7].to_vec(), ids[1..4].to_vec(), ids[0..1].to_vec()]
    );
    // Rows are 1 to 50.
    assert_eq!(dms.history(a[0], a[1], 0, 0).unwrap().len(), 1);
    assert_eq!(dms.history(a[0], a[1], 0, 255).unwrap().len(), 7);
}

#[test]
fn dm_read_marks() {
    let (db, a) = with_accounts(3);
    let dms = db.dms();
    let l = DmLimits::default();
    let mut last = 0;
    for _ in 0..3 {
        last = dms
            .send(a[1], a[0], "B", "A", "hi", T0, true, l)
            .unwrap()
            .id;
    }
    let from_c = dms
        .send(a[2], a[0], "C", "A", "yo", T0, false, l)
        .unwrap()
        .id;
    dms.send(a[0], a[1], "A", "B", "mine", T0, true, l).unwrap();
    assert_eq!(dms.unread(a[0]).unwrap(), (4, 2));
    assert_eq!(dms.unread(a[1]).unwrap(), (1, 1), "a0's line to a1");
    assert_eq!(dms.unread_from(a[0], a[1]).unwrap(), 3);
    dms.mark_read(a[0], a[1], last - 1).unwrap();
    assert_eq!(dms.unread_from(a[0], a[1]).unwrap(), 1);
    dms.mark_read(a[0], a[1], last).unwrap();
    assert_eq!(dms.unread(a[0]).unwrap(), (1, 1));
    // A mark never moves back.
    dms.mark_read(a[0], a[1], 1).unwrap();
    assert_eq!(dms.unread_from(a[0], a[1]).unwrap(), 0);
    dms.mark_read(a[0], a[2], from_c).unwrap();
    assert_eq!(dms.unread(a[0]).unwrap(), (0, 0));
    // A new line counts again.
    dms.send(a[1], a[0], "B", "A", "again", T0, true, l)
        .unwrap();
    assert_eq!(dms.unread(a[0]).unwrap(), (1, 1));
}

#[test]
fn dm_conversations_newest_first() {
    let (db, a) = with_accounts(33);
    let dms = db.dms();
    let l = DmLimits::default();
    // a0 talks with a1..a32; a1 last.
    for (i, &p) in a[1..].iter().enumerate().rev() {
        dms.send(
            p,
            a[0],
            &format!("C{i}"),
            "Me",
            "hello",
            T0 + 100 - i64::try_from(i).unwrap(),
            true,
            l,
        )
        .unwrap();
    }
    let long = format!("{}é tail", "x".repeat(59));
    dms.send(a[0], a[1], "Me", "Kashya", &long, T0 + 200, true, l)
        .unwrap();
    let (total, page0) = dms.conversations(a[0], 0).unwrap();
    assert_eq!((total, page0.len()), (32, 30));
    let c = &page0[0];
    assert_eq!(
        (
            c.peer,
            c.peer_account.as_str(),
            c.peer_char.as_str(),
            c.unread,
            c.last_ms
        ),
        (a[1], "A1", "Kashya", 1, T0 + 200)
    );
    assert_eq!(
        c.last_text,
        "x".repeat(59),
        "cut to 60 bytes on a character boundary"
    );
    assert_eq!((page0[1].peer, page0[1].peer_char.as_str()), (a[2], "C1"));
    let (_, page1) = dms.conversations(a[0], 1).unwrap();
    assert_eq!(
        page1.iter().map(|c| c.peer).collect::<Vec<_>>(),
        [a[31], a[32]]
    );
    assert_eq!(dms.conversations(a[0], 2).unwrap().1, []);
    let (total, one) = dms.conversations(a[5], 0).unwrap();
    assert_eq!(
        (total, one.len(), one[0].peer, one[0].unread),
        (1, 1, a[0], 0)
    );
}

// --- Mutes ---

#[test]
fn mutes_end() {
    let (db, a) = with_accounts(2);
    let m = db.mutes();
    assert_eq!(m.get(a[0], T0).unwrap(), None);
    m.mute(a[0], T0 + 60_000, "spam").unwrap();
    assert_eq!(
        m.get(a[0], T0).unwrap(),
        Some(Mute {
            until_ms: T0 + 60_000,
            reason: "spam".into()
        })
    );
    assert_eq!(m.get(a[0], T0 + 60_000).unwrap(), None, "ended");
    m.mute(a[0], T0 + 120_000, "").unwrap();
    assert_eq!(
        m.get(a[0], T0 + 60_000).unwrap().unwrap().until_ms,
        T0 + 120_000
    );
    m.mute(a[1], T0 + 10, "x").unwrap();
    assert_eq!(m.sweep(T0 + 10).unwrap(), 1);
    assert!(m.unmute(a[0]).unwrap());
    assert!(!m.unmute(a[0]).unwrap());
    assert_eq!(m.get(a[0], T0).unwrap(), None);
}
