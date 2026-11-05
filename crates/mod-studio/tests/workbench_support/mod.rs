//! Install-shaped tables invented solely for Studio acceptance tests and renders.
use d2_data::{
    bin::DataRules,
    patch::{self, PatchData, PatchTable},
    strings::StringTables,
};
use mod_studio::workbench::Snapshot;
use std::collections::BTreeMap;
pub fn snapshot() -> Snapshot {
    let schemas = mod_studio::tables::schemas();
    let mut texts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for rule in patch::rules_for(DataRules::Mod) {
        let mut columns: Vec<_> = schemas[&rule.name]
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        columns.retain(|c| seen.insert(c.to_ascii_lowercase()));
        if let Some(key) = &rule.key_field {
            let key = String::from_utf8_lossy(key).into_owned();
            if !columns.iter().any(|c| c.eq_ignore_ascii_case(&key)) {
                columns.push(key);
            }
        }
        texts.insert(
            rule.name,
            format!(
                "{}\r\n{}\r\n",
                columns.join("\t"),
                vec![""; columns.len()].join("\t")
            )
            .into_bytes(),
        );
    }
    texts.insert("weapons".into(),b"name\tcode\ttype\tnamestr\tmindam\tmaxdam\tspawnable\tlevel\tversion\tnodurability\tnormcode\tubercode\tultracode\thasinv\r\nTest blade\tqa1\tweap\tqa1\t3\t8\t1\t1\t100\t1\tqa1\tqbu\tqel\t1\r\nTest wand\tqa2\tweap\tqa2\t1\t5\t1\t1\t100\t1\tqa2\tqbu\tqel\t1\r\n".to_vec());
    texts.insert(
        "automap".into(),
        b"LevelName\tTileName\r\n1 Town\tfl\r\n".to_vec(),
    );
    let mut modes = String::from("name\tcode\r\n");
    for i in 0..16 {
        modes.push_str(&format!("Test mode {i}\tm{i:02}\r\n"));
    }
    texts.insert("monmode".into(), modes.into_bytes());
    let mut types = String::from("ItemType\tCode\tEquiv1\tNormal\tClass\tRarity\r\n");
    for i in 0..60 {
        let code = if i == 45 {
            "weap".into()
        } else {
            format!("t{i:02}")
        };
        types.push_str(&format!("Test type {i}\t{code}\t\t1\t255\t3\r\n"));
    }
    texts.insert("itemtypes".into(), types.into_bytes());
    texts.insert("treasureclassex".into(),b"Treasure Class\tPicks\tNoDrop\tItem1\tProb1\tItem2\tProb2\r\nTest pool\t1\t1\tqa1\t2\tqa2\t1\r\n".to_vec());
    texts.insert("itemratio".into(),b"Version\tUber\tClass Specific\tUnique\tUniqueDivisor\tUniqueMin\tSet\tSetDivisor\tSetMin\tRare\tRareDivisor\tRareMin\tMagic\tMagicDivisor\tMagicMin\tHiQuality\tHiQualityDivisor\tNormal\tNormalDivisor\r\n100\t0\t0\t400\t1\t6400\t160\t2\t5600\t100\t2\t3200\t34\t3\t192\t12\t8\t2\t2\r\n".to_vec());
    texts.insert("cubemain".into(),b"description\tenabled\tversion\tnuminputs\tinput 1\toutput\tlvl\r\nTest conversion\t1\t100\t1\tqa1\tqa2\t10\r\n".to_vec());
    texts.insert("runes".into(),b"Name\tcomplete\titype1\tRune1\tRune2\tT1Code1\tT1Min1\tT1Max1\r\nTestword\t1\tweap\tqa2\tqa2\tmk-damage\t4\t4\r\n".to_vec());
    texts.insert(
        "properties".into(),
        b"code\tfunc1\tstat1\r\nmk-damage\t1\ttest_stat_21\r\n".to_vec(),
    );
    for name in ["magicprefix", "magicsuffix", "automagic"] {
        texts.insert(
            name.into(),
            b"Name\tmod1code\tmod1min\tmod1max\r\nTestaffix\tmk-damage\t4\t4\r\n".to_vec(),
        );
    }
    let mut stats = String::from("Stat\tID\tSave Bits\tSave Add\tValShift\r\n");
    for i in 0..196 {
        stats.push_str(&format!("test_stat_{i}\t{i}\t16\t0\t0\r\n"));
    }
    texts.insert("itemstatcost".into(), stats.into_bytes());
    let tables = patch::rules_for(DataRules::Mod)
        .into_iter()
        .map(|rule| PatchTable::from_base(&rule, "synthetic", &texts[&rule.name]).unwrap())
        .collect();
    Snapshot {
        drop_rules: d2_sim::treasure::rules::DropRules::defaults(),
        data: PatchData::from_tables(tables),
        native: None,
        strings: strings(),
    }
}
fn strings() -> StringTables {
    use d2_formats::tbl::{key_hash, StringTable, TblEntry, TblHeader};
    let entries: [(&[u8], u16, &[u8]); 10] = [
        (b"qa1", 1, b"Test blade"),
        (b"qa2", 2, b"Test wand"),
        (b"Testword", 3, b"Testword"),
        (b"Testaffix", 4, b"Testaffix"),
        (b"damage", 3465, b"Damage:"),
        (b"space", 3995, b" "),
        (b"dash", 3996, b"-"),
        (b"colon", 3997, b":"),
        (b"newline", 3998, b"\n"),
        (b"to", 3464, b"to"),
    ];
    let slots = 17;
    let mut rows = vec![
        TblEntry {
            used: 0,
            index: 0,
            hash: 0,
            key: vec![],
            value: vec![]
        };
        slots
    ];
    let mut indices = vec![u16::MAX; 4000];
    for (key, id, value) in entries {
        let mut slot = (key_hash(key) % slots as u32) as usize;
        while rows[slot].used != 0 {
            slot = (slot + 1) % slots;
        }
        rows[slot] = TblEntry {
            used: 1,
            index: id,
            hash: key_hash(key),
            key: key.to_vec(),
            value: value.to_vec(),
        };
        indices[id as usize] = slot as u16;
    }
    StringTables {
        base: Some(StringTable {
            header: TblHeader {
                crc: 0,
                num_elements: 4000,
                hash_table_size: slots as u32,
                version: 0,
                data_start: 0,
                max_tries: slots as u32,
                file_size: 0,
            },
            indices,
            entries: rows,
        }),
        ..Default::default()
    }
}
