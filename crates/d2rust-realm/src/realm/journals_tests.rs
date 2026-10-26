//! The journal store on chunks made with the journal format, in a temp
//! folder: order, waiting chunks, broken journals, file names.

use d2_journal::chunk::compress;
use d2_journal::format::{encode_body, Entry, Record, Start};
use d2_journal::read_journal;

use super::*;

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-realm-journals")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn store(dir: &Path) -> Journals {
    let log = Arc::new(ServerLog::open_named(&dir.join("logs"), "realm.log").unwrap());
    Journals::new(dir.join("journals"), log)
}

/// Chunk `seq` of game `game`: chunk 0 starts with `Start`; `end` closes it.
fn chunk(game: u32, seq: u32, end: bool) -> Vec<u8> {
    let tick = seq * 600;
    let mut entries = Vec::new();
    if seq == 0 {
        let mut start = Start::default();
        start.spec.id = game;
        entries.push(Entry {
            tick,
            record: Record::Start(start),
        });
    }
    entries.push(Entry {
        tick: tick + 1,
        record: Record::Frame { host_ms: tick },
    });
    if end {
        entries.push(Entry {
            tick: tick + 2,
            record: Record::End { reason: 0 },
        });
    }
    compress(&encode_body(game, seq, &entries))
}

/// The journal files under `dir`, in path order.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for day in std::fs::read_dir(dir.join("journals")).unwrap() {
        for f in std::fs::read_dir(day.unwrap().path()).unwrap() {
            out.push(f.unwrap().path());
        }
    }
    out.sort();
    out
}

fn log_of(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("logs").join("realm.log")).unwrap_or_default()
}

#[test]
fn chunks_in_order_make_a_journal_that_reads_back() {
    let dir = temp("order");
    let j = store(&dir);
    assert_eq!(j.chunk("n1", 4, 0, chunk(4, 0, false)), Ok(Kept::Stored));
    assert_eq!(j.chunk("n1", 4, 1, chunk(4, 1, false)), Ok(Kept::Stored));
    assert_eq!(j.chunk("n1", 4, 2, chunk(4, 2, true)), Ok(Kept::Complete));
    let f = files(&dir);
    assert_eq!(f.len(), 1);
    assert!(f[0].ends_with("4.d2j.gz"), "{}", f[0].display());
    let journal = read_journal(&std::fs::read(&f[0]).unwrap()).unwrap();
    assert_eq!((journal.game, journal.chunks, journal.ended), (4, 3, true));
    assert!(log_of(&dir).contains("game 4: journal complete, 3 chunks"));
}

#[test]
fn an_early_chunk_waits_for_the_one_before_it() {
    let dir = temp("early");
    let j = store(&dir);
    assert_eq!(j.chunk("n1", 2, 0, chunk(2, 0, false)), Ok(Kept::Stored));
    assert_eq!(j.chunk("n1", 2, 2, chunk(2, 2, true)), Ok(Kept::Waiting));
    // Chunk 1 lets the waiting last one follow.
    assert_eq!(j.chunk("n1", 2, 1, chunk(2, 1, false)), Ok(Kept::Complete));
    let journal = read_journal(&std::fs::read(&files(&dir)[0]).unwrap()).unwrap();
    assert_eq!((journal.chunks, journal.ended), (3, true));
}

#[test]
fn a_bad_chunk_ends_the_journal_and_keeps_the_chunks_before() {
    let dir = temp("bad");
    let j = store(&dir);
    j.chunk("n1", 3, 0, chunk(3, 0, false)).unwrap();
    let why = j.chunk("n1", 3, 1, b"not gzip".to_vec()).unwrap_err();
    assert!(why.contains("gzip"), "{why}");
    assert_eq!(j.chunk("n1", 3, 2, chunk(3, 2, true)), Ok(Kept::Dropped));
    assert!(log_of(&dir).contains("game 3: journal invalid at chunk 1"));
    let journal = read_journal(&std::fs::read(&files(&dir)[0]).unwrap()).unwrap();
    assert_eq!((journal.chunks, journal.ended), (1, false));

    // Another game's chunk under this game's number.
    j.chunk("n1", 5, 0, chunk(5, 0, false)).unwrap();
    let why = j.chunk("n1", 5, 1, chunk(6, 1, false)).unwrap_err();
    assert!(why.contains("of game 6"), "{why}");
    // A chunk sent twice.
    j.chunk("n1", 7, 0, chunk(7, 0, false)).unwrap();
    assert!(j.chunk("n1", 7, 0, chunk(7, 0, false)).is_err());
}

#[test]
fn a_gap_no_chunk_fills_ends_the_journal() {
    let dir = temp("gap");
    let j = store(&dir);
    j.chunk("n1", 1, 0, chunk(1, 0, false)).unwrap();
    for seq in 2..2 + MAX_WAITING as u32 {
        assert_eq!(
            j.chunk("n1", 1, seq, chunk(1, seq, false)),
            Ok(Kept::Waiting)
        );
    }
    let why = j.chunk("n1", 1, 99, chunk(1, 99, false)).unwrap_err();
    assert!(why.contains("chunk 1 never came"), "{why}");
}

#[test]
fn a_game_number_seen_before_today_gets_a_file_of_its_own() {
    let dir = temp("again");
    let j = store(&dir);
    j.chunk("n1", 1, 0, chunk(1, 0, true)).unwrap();
    // After a restart, game ids start at 1 again.
    let j = store(&dir);
    j.chunk("n1", 1, 0, chunk(1, 0, true)).unwrap();
    let names: Vec<String> = files(&dir)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["1-2.d2j.gz", "1.d2j.gz"]);
}

#[test]
fn a_lost_game_server_s_journals_end_where_they_are() {
    let dir = temp("lost");
    let j = store(&dir);
    j.chunk("n1", 8, 0, chunk(8, 0, false)).unwrap();
    j.chunk("n2", 9, 0, chunk(9, 0, false)).unwrap();
    j.node_lost("n1");
    assert!(log_of(&dir).contains("node n1: game 8: journal cut off after 1 chunks"));
    // A chunk after the loss has no journal to go to; n2's goes on.
    assert_eq!(j.chunk("n1", 8, 1, chunk(8, 1, true)), Ok(Kept::Dropped));
    assert_eq!(j.chunk("n2", 9, 1, chunk(9, 1, true)), Ok(Kept::Complete));
}
