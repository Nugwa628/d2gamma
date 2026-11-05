//! Small invented tables shared by acceptance tests and framebuffer captures.
use d2_data::{
    bin::DataRules,
    patch::{self, PatchData, PatchTable},
};
pub fn data() -> PatchData {
    let rules = patch::rules_for(DataRules::Mod);
    let table = |name: &str, text: &[u8]| {
        PatchTable::from_base(
            rules.iter().find(|r| r.name == name).unwrap(),
            "synthetic",
            text,
        )
        .unwrap()
    };
    PatchData::from_tables(vec![
        table("weapons",b"name\tcode\ttype\tlevel\tmindam\tmaxdam\tspawnable\r\nEmber blade\teb1\tblad\t5\t3\t8\t1\r\nFrost spear\tfs1\tsper\t12\t8\t17\t1\r\nStorm staff\tss1\tstaf\t20\t4\t23\t1\r\n"),
        table("itemtypes",b"name\tcode\tequiv1\trepair\r\nBlades\tblad\t\t1\r\nSpears\tsper\t\t1\r\nStaves\tstaf\t\t1\r\n"),
    ])
}
