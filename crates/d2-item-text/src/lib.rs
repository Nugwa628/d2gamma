//! The engine item description builder shared by the game and Mod Studio.
pub mod item_tip;
pub mod item_tip_build;
pub mod item_tip_desc;
pub mod item_tip_props;
pub mod item_tip_set;
pub mod lookup;
pub mod wformat;
pub trait StringLookup {
    fn get(&self, key: &str) -> Option<&[u16]>;

    /// Lookup by string-table id (panels name some strings by id). The
    /// game's tables answer it (`app::strings::TableStrings`); None here.
    fn get_id(&self, _id: u16) -> Option<&[u16]> {
        None
    }
}
