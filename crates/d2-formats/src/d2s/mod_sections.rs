//! A mod game's own save data: tagged sections a character save carries
//! next to the engine's own ([`super::Body::mod_sections`]).
//!
//! The engine never reads their bytes. A mod game fills them at save time
//! and gets them back when the character joins a game (the `SaveHooks` of
//! its rules), and its own save codec stores them. A `.d2s` file has no
//! place for them: its reader leaves the list empty and its writer refuses
//! a body that holds any.

/// One mod section: a tag the mod game chooses and the section's bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModSection {
    /// The section's 4-byte tag; each tag appears once in a save.
    pub tag: [u8; 4],
    /// The section's bytes, in the mod game's own layout.
    pub data: Vec<u8>,
}

impl ModSection {
    /// A section `tag` holding `data`.
    pub fn new(tag: [u8; 4], data: Vec<u8>) -> ModSection {
        ModSection { tag, data }
    }
}

/// The section of `sections` tagged `tag`, if any.
pub fn find<'a>(sections: &'a [ModSection], tag: &[u8; 4]) -> Option<&'a ModSection> {
    sections.iter().find(|s| &s.tag == tag)
}
