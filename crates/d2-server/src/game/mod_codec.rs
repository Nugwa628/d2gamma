//! The data mod character codec. Base games retain D2sCodec.
use super::{CharacterCodec, LiveData};
use d2_formats::d2s::{self, D2s, SaveTables, StatSave};
use d2_sim::rules::Ruleset;

#[derive(Clone, Debug)]
pub struct ModCodec(pub Ruleset);

/// Copy a plain character into the kit's wide save format. Source item
/// records must be decoded with plain tables before their stat ids widen.
pub fn copy_d2gamma_character(
    bytes: &[u8],
    tables: &crate::world_data::tables::SaveData,
) -> Result<Vec<u8>, String> {
    use d2_sim::items::bitstream::{
        read::{read_save_entry, ReadEntry},
        write_save, StreamItem,
    };
    fn tree(entry: ReadEntry) -> Result<StreamItem, String> {
        if entry.item.failed {
            return Err("character contains an invalid item".into());
        }
        let mut item = entry.item.item;
        item.children = entry
            .children
            .into_iter()
            .map(tree)
            .collect::<Result<_, _>>()?;
        Ok(item)
    }
    let mut plain = tables.clone();
    plain.items.rules = Ruleset::Original;
    let expansion = d2s::header_status(bytes).is_some_and(|s| s & d2s::status::EXPANSION != 0);
    let mut save = d2s::read(
        bytes,
        &d2s::ReadOptions {
            expansion,
            game: None,
        },
        &plain,
    )
    .map_err(|e| e.to_string())?;
    let mut wide = plain.clone();
    wide.items.rules = d2_sim::rules::DataMod::new("character copy").into_ruleset();
    let convert = |item: &mut d2s::ItemEntry| -> Result<(), String> {
        let entry = read_save_entry(&item.bytes, &plain.items).map_err(|e| e.to_string())?;
        if entry.len != item.bytes.len() {
            return Err("character item contains trailing data".into());
        }
        item.bytes = write_save(&tree(entry)?, &wide.items)
            .map_err(|e| e.to_string())?
            .0;
        Ok(())
    };
    if let Some(body) = &mut save.body {
        for item in body
            .items
            .iter_mut()
            .chain(body.corpses.iter_mut().flat_map(|c| &mut c.items))
            .chain(body.hireling_items.iter_mut().flatten().flatten())
            .chain(body.golem.iter_mut().filter_map(|g| g.item.as_mut()))
        {
            convert(item)?;
        }
    }
    let result = d2_formats::d2m::write(&save, &wide)?;
    d2_formats::d2m::read(
        &result,
        &d2s::ReadOptions {
            expansion,
            game: None,
        },
        &wide,
    )?;
    Ok(result)
}
struct Tables<'a> {
    base: &'a dyn SaveTables,
    rules: &'a Ruleset,
}
impl SaveTables for Tables<'_> {
    fn wide_stats(&self) -> bool {
        true
    }
    fn stat_save(&self, id: u16) -> Option<StatSave> {
        if self
            .rules
            .stat_rules()
            .is_some_and(|r| r.custom_stats().any(|(_, stat)| stat == id))
        {
            Some(StatSave {
                bits: 32,
                param: 16,
                signed: true,
            })
        } else {
            self.base.stat_save(id)
        }
    }
    fn item_entry_len(&self, bytes: &[u8]) -> Result<usize, String> {
        let custom: Vec<_> = self
            .rules
            .stat_rules()
            .into_iter()
            .flat_map(|r| r.custom_stats().map(|(_, id)| id))
            .collect();
        self.base.wide_item_entry_len(bytes, &custom)
    }
    fn hireling_restored(&self, h: &d2s::Hireling) -> bool {
        self.base.hireling_restored(h)
    }
}
impl CharacterCodec for ModCodec {
    fn extension(&self) -> &str {
        "d2m"
    }
    fn read(&self, data: &LiveData, bytes: &[u8], difficulty: u8) -> Result<D2s, String> {
        if !self
            .0
            .difficulties()
            .map_or(difficulty < 3, |d| d.contains(difficulty))
        {
            return Err("difficulty outside the configured mod list".into());
        }
        let mut base = data.save.clone();
        base.items.rules = self.0.clone();
        let options = d2s::ReadOptions {
            expansion: super::GAME_SETUP.expansion,
            game: Some(d2s::GameContext {
                client_name: d2s::header_name(bytes).to_vec(),
                expansion: super::GAME_SETUP.expansion,
                hardcore: d2s::header_status(bytes).is_some_and(|s| s & d2s::status::HARDCORE != 0),
                difficulty: if self.0.difficulties().is_some() {
                    0
                } else {
                    difficulty
                },
            }),
        };
        d2_formats::d2m::read(
            bytes,
            &options,
            &Tables {
                base: &base,
                rules: &self.0,
            },
        )
    }
    fn write(&self, save: &D2s, tables: &dyn SaveTables) -> Result<Vec<u8>, String> {
        d2_formats::d2m::write(
            save,
            &Tables {
                base: tables,
                rules: &self.0,
            },
        )
    }
}
