//! The animation table (`animdata.d2`) as `P.tsv`.
//!
//! The table gives every unit animation (named by its COF, such as
//! `AIA1HTH`) its frame count, its speed, and the event on each frame: the
//! frame an attack hits, a missile leaves the hand, a sound plays. The game
//! looks records up through a hash of the name into 256 buckets.
//!
//! The TSV has one row per record, bucket by bucket, in file order. Order
//! matters: some names appear twice, and a lookup returns the first one in
//! its bucket. Events are written as `frame:code` pairs for the frames that
//! have one. [`check_animdata`] proves the TSV rebuilds the same records.
//!
//! A record sits in the bucket its name hashes to, so the row needs no
//! bucket. Some tools file a record elsewhere (under the hash of another
//! spelling of its name, such as the uppercase one); a file holding such a
//! record gets a fifth column, `bucket`, on every row, and is read back with
//! each record in the bucket it names. Only such a file has the column, so
//! each file has one spelling.

use d2_formats::animdata::{hash, AnimData, AnimRecord, BUCKETS, EVENTS};

use crate::toml_kinds::{
    check_round_trip, latin1, tsv_escape, tsv_rows, tsv_unescape, unlatin1, TextError,
};

const TSV_HEADER: &str = "cof\tframes\tspeed\tevents";
/// The header of a file with records outside their name's bucket.
const TSV_HEADER_BUCKETS: &str = "cof\tframes\tspeed\tevents\tbucket";

/// The length of a record name: up to its first NUL.
fn name_len(r: &AnimRecord) -> usize {
    r.name.iter().position(|&c| c == 0).unwrap_or(r.name.len())
}

/// Writes `P.tsv`. A record whose name has non-zero bytes after its first
/// NUL cannot be written (the row has no place for them); that is an
/// error, never a silent loss.
pub fn write_animdata(file: &str, a: &AnimData) -> Result<String, TextError> {
    let bad = |d: String| TextError::new(file, format!("not representable: {d}"));
    if a.buckets.len() != BUCKETS {
        return Err(bad(format!("{} buckets", a.buckets.len())));
    }
    let elsewhere = a
        .buckets
        .iter()
        .enumerate()
        .any(|(b, bucket)| bucket.iter().any(|r| hash(&r.name[..name_len(r)]) != b));
    let mut o = String::from(if elsewhere {
        TSV_HEADER_BUCKETS
    } else {
        TSV_HEADER
    });
    o.push('\n');
    for (b, bucket) in a.buckets.iter().enumerate() {
        for r in bucket {
            let len = name_len(r);
            if r.name[len..].iter().any(|&c| c != 0) {
                return Err(bad(format!("bucket {b}: name bytes after the first NUL")));
            }
            if len == r.name.len() {
                return Err(bad(format!("bucket {b}: name has no NUL")));
            }
            let events: Vec<String> = r
                .events
                .iter()
                .enumerate()
                .filter(|(_, &e)| e != 0)
                .map(|(f, e)| format!("{f}:{e}"))
                .collect();
            o.push_str(&format!(
                "{}\t{}\t{}\t{}",
                tsv_escape(&latin1(&r.name[..len])),
                r.frames,
                r.speed,
                events.join(" ")
            ));
            if elsewhere {
                o.push_str(&format!("\t{b}"));
            }
            o.push('\n');
        }
    }
    Ok(o)
}

/// Reads `P.tsv` back; each record goes to the bucket its name hashes to
/// (or the one its `bucket` cell names), in row order.
pub fn read_animdata(file: &str, tsv: &str) -> Result<AnimData, TextError> {
    let mut buckets: Vec<Vec<AnimRecord>> = vec![Vec::new(); BUCKETS];
    let with_buckets = tsv.split('\n').next() == Some(TSV_HEADER_BUCKETS);
    let (header, columns) = if with_buckets {
        (TSV_HEADER_BUCKETS, 5)
    } else {
        (TSV_HEADER, 4)
    };
    let (mut last_bucket, mut elsewhere) = (0, false);
    for (i, cells) in tsv_rows(file, tsv, header, columns)?
        .into_iter()
        .enumerate()
    {
        let line = i + 2;
        let err = |d: String| TextError::new(file, format!("line {line}: {d}"));
        let name = unlatin1(file, "cof", &tsv_unescape(file, line, cells[0])?)?;
        if name.len() > 7 || name.contains(&0) {
            return Err(err("name is over 7 bytes or holds a NUL".into()));
        }
        let num = |s: &str, what: &str| {
            // One spelling per value: plain decimal, no sign, no leading zero.
            if s.is_empty()
                || (s.len() > 1 && s.starts_with('0'))
                || !s.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(err(format!("{what} {s:?} is not a plain decimal")));
            }
            s.parse::<u32>()
                .map_err(|_| err(format!("{what} {s:?} out of range")))
        };
        let frames = num(cells[1], "frames")?;
        let speed = num(cells[2], "speed")?;
        let mut events = [0u8; EVENTS];
        if !cells[3].is_empty() {
            let mut last = None;
            for pair in cells[3].split(' ') {
                let (f, c) = pair
                    .split_once(':')
                    .ok_or_else(|| err(format!("event {pair:?} is not frame:code")))?;
                let f = num(f, "event frame")? as usize;
                let c = num(c, "event code")?;
                if f >= EVENTS || c == 0 || c > 255 {
                    return Err(err(format!("event {pair:?} out of range")));
                }
                if last.is_some_and(|l| f <= l) {
                    return Err(err("events are not in increasing frame order".into()));
                }
                last = Some(f);
                events[f] = c as u8;
            }
        }
        let bucket = if with_buckets {
            let b = num(cells[4], "bucket")? as usize;
            if b >= BUCKETS || b < last_bucket {
                return Err(err(format!(
                    "bucket {b} is out of range or before the previous row's"
                )));
            }
            last_bucket = b;
            elsewhere |= b != hash(&name);
            b
        } else {
            hash(&name)
        };
        let mut padded = [0u8; 8];
        padded[..name.len()].copy_from_slice(&name);
        buckets[bucket].push(AnimRecord {
            name: padded,
            frames,
            speed,
            events,
        });
    }
    if with_buckets && !elsewhere {
        return Err(TextError::new(
            file,
            "a `bucket` column, but every record is in its name's bucket",
        ));
    }
    Ok(AnimData { buckets })
}

/// Round-trip check for animdata: write, read back, compare the records.
pub fn check_animdata(file: &str, original: &AnimData) -> Result<(), TextError> {
    check_round_trip(file, original, write_animdata, read_animdata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toml_kinds::first_difference;

    fn rec(name: &str, frames: u32, speed: u32, events: &[(usize, u8)]) -> AnimRecord {
        let mut n = [0u8; 8];
        n[..name.len()].copy_from_slice(name.as_bytes());
        let mut e = [0u8; EVENTS];
        for &(f, c) in events {
            e[f] = c;
        }
        AnimRecord {
            name: n,
            frames,
            speed,
            events: e,
        }
    }

    fn sample() -> AnimData {
        let mut buckets = vec![Vec::new(); BUCKETS];
        for r in [
            rec("AIA1HTH", 12, 256, &[(3, 1), (143, 4)]),
            rec("AIA1HTH", 99, 100, &[]), // duplicate name, second in its bucket
            rec("A", 1, 2, &[(0, 3)]),
            rec("", 4, 5, &[]),
        ] {
            buckets[hash(&r.name)].push(r);
        }
        AnimData { buckets }
    }

    #[test]
    fn round_trip_keeps_file_order_and_duplicates() {
        let a = sample();
        let t = write_animdata("a.tsv", &a).unwrap();
        assert!(t.starts_with("cof\tframes\tspeed\tevents\n"));
        assert!(t.contains("AIA1HTH\t12\t256\t3:1 143:4\nAIA1HTH\t99\t100\t\n"));
        assert_eq!(read_animdata("a.tsv", &t).unwrap(), a);
        check_animdata("a.tsv", &a).unwrap();
        assert_eq!(t, write_animdata("a.tsv", &a).unwrap());
        // Bucket order: "" (hash 0) first.
        assert!(t.lines().nth(1).unwrap().starts_with("\t4\t5"));
    }

    #[test]
    fn unrepresentable_records_are_errors() {
        let mut a = sample();
        a.buckets[hash(b"A")][0].name = *b"A\0\0\0\0\0\0x";
        assert!(write_animdata("a.tsv", &a).is_err());
    }

    #[test]
    fn records_outside_their_bucket_keep_their_place() {
        // A record filed under the hash of another spelling of its name (a
        // lowercase name in its uppercase name's bucket).
        let mut a = sample();
        let mut r = rec("AIA1HTH", 7, 8, &[(1, 2)]);
        r.name[0] = b'a';
        let home = hash(b"AIA1HTH");
        assert_ne!(hash(&r.name[..7]), home);
        a.buckets[home].push(r);
        let t = write_animdata("a.tsv", &a).unwrap();
        assert!(t.starts_with("cof\tframes\tspeed\tevents\tbucket\n"), "{t}");
        assert!(t.contains(&format!("aIA1HTH\t7\t8\t1:2\t{home}\n")), "{t}");
        assert_eq!(read_animdata("a.tsv", &t).unwrap(), a);
        check_animdata("a.tsv", &a).unwrap();
        // A file with every record at home has no bucket column, and a file
        // with the column must need it.
        let plain = write_animdata("a.tsv", &sample()).unwrap();
        assert!(plain.starts_with("cof\tframes\tspeed\tevents\n"));
        let homes: Vec<usize> = sample()
            .buckets
            .iter()
            .enumerate()
            .flat_map(|(b, bucket)| bucket.iter().map(move |_| b))
            .collect();
        let mut forced = String::from("cof\tframes\tspeed\tevents\tbucket\n");
        for (line, b) in plain.lines().skip(1).zip(&homes) {
            forced += &format!("{line}\t{b}\n");
        }
        assert!(read_animdata("a.tsv", &forced).is_err());
        // Buckets stay in range and run in file order.
        for bad in [
            t.replace(&format!("\t{home}\n"), "\t256\n"),
            t.replacen("\t0\n", "\t255\n", 1),
        ] {
            assert!(read_animdata("a.tsv", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn perturbation_and_strict_reader() {
        let a = sample();
        let t = write_animdata("a.tsv", &a).unwrap();
        let p = t.replace("\t12\t", "\t13\t");
        let back = read_animdata("a.tsv", &p).unwrap();
        assert_ne!(back, a);
        assert!(first_difference(&a, &back).contains("12"));
        for bad in [
            t.replace("3:1", "3:0"),
            t.replace("3:1 143:4", "143:4 3:1"),
            t.replace("3:1", "144:1"),
            t.replace("\t12\t", "\t012\t"),
            t.replace("A\t1\t2", "AAAAAAAA\t1\t2"),
            t.replace("cof\t", "name\t"),
            t.trim_end().to_owned(),
        ] {
            assert!(read_animdata("a.tsv", &bad).is_err(), "{bad}");
        }
    }
}
