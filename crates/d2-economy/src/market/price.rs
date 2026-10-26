//! Prices (contracts 11.13, `Price`): gold, counts of stackables (the
//! market's currencies) and, in offers and counter-offers only, specific
//! stash items. One canonical byte form is stored and hashed, so the hash
//! a buyer saw names exactly one price.

use sha2::{Digest, Sha256};

use crate::config::EconomyConfig;
use crate::error::Refusal;
use crate::key::ItemId;

/// A price or an offer's bundle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Price {
    pub gold: u64,
    /// `(stackable code, count)`.
    pub stacks: Vec<(String, u32)>,
    /// Stash items (offers and counters only).
    pub items: Vec<ItemId>,
}

/// The version byte of the canonical form.
const FORM: u8 = 1;

impl Price {
    pub fn gold(gold: u64) -> Self {
        Price {
            gold,
            ..Price::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.gold == 0 && self.stacks.is_empty() && self.items.is_empty()
    }

    /// The same price with each code once (counts added up), codes and
    /// items in order, and no zero counts. Refused if a count overflows or
    /// an item is named twice.
    pub fn normalized(&self) -> Result<Price, Refusal> {
        let mut stacks: Vec<(String, u32)> = Vec::new();
        let mut sorted = self.stacks.clone();
        sorted.sort();
        for (code, count) in sorted {
            if count == 0 {
                continue;
            }
            match stacks.last_mut() {
                Some((c, n)) if *c == code => {
                    *n = n.checked_add(count).ok_or(Refusal::BadRequest)?;
                }
                _ => stacks.push((code, count)),
            }
        }
        let mut items = self.items.clone();
        items.sort();
        let len = items.len();
        items.dedup();
        if items.len() != len {
            return Err(Refusal::BadRequest);
        }
        Ok(Price {
            gold: self.gold,
            stacks,
            items,
        })
    }

    /// Checks a price for the market: not empty, gold that fits the
    /// database, codes of the realm's stackables, and items only where
    /// `items_allowed`. Returns it normalized.
    pub(crate) fn checked(
        &self,
        config: &EconomyConfig,
        items_allowed: bool,
    ) -> Result<Price, Refusal> {
        let p = self.normalized()?;
        if p.is_empty()
            || i64::try_from(p.gold).is_err()
            || (!items_allowed && !p.items.is_empty())
            || p.stacks.iter().any(|(c, _)| config.stackable(c).is_none())
            || p.stacks.len() > usize::from(u8::MAX)
            || p.items.len() > usize::from(u8::MAX)
        {
            return Err(Refusal::BadRequest);
        }
        Ok(p)
    }

    /// The canonical bytes: form, gold, then each stack and item in order.
    pub fn encode(&self) -> Vec<u8> {
        let p = self.normalized().unwrap_or_else(|_| self.clone());
        let mut out = vec![FORM];
        out.extend_from_slice(&p.gold.to_le_bytes());
        out.push(p.stacks.len() as u8);
        for (code, count) in &p.stacks {
            out.push(code.len() as u8);
            out.extend_from_slice(code.as_bytes());
            out.extend_from_slice(&count.to_le_bytes());
        }
        out.push(p.items.len() as u8);
        for id in &p.items {
            out.extend_from_slice(&id.0.to_le_bytes());
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Price> {
        fn take<'a>(r: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
            if r.len() < n {
                return None;
            }
            let (a, b) = r.split_at(n);
            *r = b;
            Some(a)
        }
        let r = &mut &*bytes;
        if take(r, 1)? != [FORM] {
            return None;
        }
        let gold = u64::from_le_bytes(take(r, 8)?.try_into().ok()?);
        let n = take(r, 1)?[0];
        let mut stacks = Vec::with_capacity(n.into());
        for _ in 0..n {
            let len = take(r, 1)?[0];
            let code = String::from_utf8(take(r, len.into())?.to_vec()).ok()?;
            let count = u32::from_le_bytes(take(r, 4)?.try_into().ok()?);
            stacks.push((code, count));
        }
        let m = take(r, 1)?[0];
        let mut items = Vec::with_capacity(m.into());
        for _ in 0..m {
            items.push(ItemId(u32::from_le_bytes(take(r, 4)?.try_into().ok()?)));
        }
        if !r.is_empty() {
            return None;
        }
        Some(Price {
            gold,
            stacks,
            items,
        })
    }

    /// `MarketBuy.price_hash`: SHA-256 of the canonical bytes.
    pub fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.encode()).into()
    }
}
