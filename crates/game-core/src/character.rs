use std::collections::{BTreeMap, BTreeSet};

use hollowrite_protocol::{
    Appearance, Archetype, Attribute, CharacterSheet, DerivedStats, EquipSlot, Equipment, ItemStack, QuestView,
};

use crate::content::{Content, ItemKind};
use crate::geom::Pos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Remnant {
    pub pos: Pos,
    pub amount: u64,
}

/// The persisted character aggregate. The world owns a live copy while the
/// character is online and hands snapshots of it to persistence; every save
/// writes the whole aggregate in one transaction, so currency, remnant and
/// inventory can never be observed half-updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterData {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub archetype: Archetype,
    pub look: Appearance,
    pub attrs: [u32; 5],
    pub echoes: u64,
    pub hp: u32,
    pub draughts: u8,
    pub draughts_max: u8,
    pub pos: Pos,
    pub checkpoint: String,
    pub lanterns: BTreeSet<String>,
    pub flags: BTreeSet<String>,
    pub inventory: BTreeMap<String, u32>,
    pub equipment: Equipment,
    pub remnant: Option<Remnant>,
    pub version: i64,
}

impl CharacterData {
    pub fn new(
        content: &Content,
        id: String,
        account_id: String,
        name: String,
        archetype: Archetype,
        look: Appearance,
    ) -> Self {
        let def = content.archetype(archetype);
        let start = content.map.lantern(&content.map.start_lantern).expect("start lantern validated at load");
        let mut inventory = BTreeMap::new();
        let mut give = |item: &str, qty: u32| *inventory.entry(item.to_string()).or_insert(0) += qty;
        give(&def.main, 1);
        give(&def.armor, 1);
        if let Some(off) = &def.off {
            give(off, 1);
        }
        if let Some(s) = &def.spell1 {
            give(s, 1);
        }
        for (item, qty) in &def.items {
            give(item, *qty);
        }
        let mut lanterns = BTreeSet::new();
        lanterns.insert(start.id.clone());
        let mut flags = BTreeSet::new();
        flags.insert(format!("lit:{}", start.id));
        let mut c = CharacterData {
            id,
            account_id,
            name,
            archetype,
            look,
            attrs: def.attrs,
            echoes: 0,
            hp: 0,
            draughts: content.balance.draughts_base,
            draughts_max: content.balance.draughts_base,
            pos: Pos::cell_center(start.x + 1, start.y),
            checkpoint: start.id.clone(),
            lanterns,
            flags,
            inventory,
            equipment: Equipment {
                main: Some(def.main.clone()),
                off: def.off.clone(),
                armor: Some(def.armor.clone()),
                spell1: def.spell1.clone(),
                spell2: None,
            },
            remnant: None,
            version: 0,
        };
        c.hp = c.derived(content).hp_max;
        c
    }

    pub fn attr(&self, a: Attribute) -> u32 {
        self.attrs[attr_index(a)]
    }

    pub fn level(&self, content: &Content) -> u32 {
        let base: u32 = content.archetype(self.archetype).attrs.iter().sum();
        let now: u32 = self.attrs.iter().sum();
        1 + now.saturating_sub(base)
    }

    pub fn next_level_cost(&self, content: &Content) -> u64 {
        level_cost(content, self.level(content))
    }

    pub fn has_item(&self, item: &str) -> bool {
        self.inventory.get(item).copied().unwrap_or(0) > 0
    }

    pub fn equipped(&self, slot: EquipSlot) -> Option<&String> {
        match slot {
            EquipSlot::Main => self.equipment.main.as_ref(),
            EquipSlot::Off => self.equipment.off.as_ref(),
            EquipSlot::Armor => self.equipment.armor.as_ref(),
            EquipSlot::Spell1 => self.equipment.spell1.as_ref(),
            EquipSlot::Spell2 => self.equipment.spell2.as_ref(),
        }
    }

    pub fn derived(&self, content: &Content) -> DerivedStats {
        let b = &content.balance.base;
        let vit = self.attr(Attribute::Vitality);
        let end = self.attr(Attribute::Endurance);
        let mig = self.attr(Attribute::Might);
        let fin = self.attr(Attribute::Finesse);
        let occ = self.attr(Attribute::Occult);
        let defense = self
            .equipment
            .armor
            .as_deref()
            .and_then(|a| content.item(a))
            .and_then(|i| i.armor.as_ref())
            .map_or(0, |a| a.defense);
        let (light, heavy) = match self.equipment.main.as_deref().and_then(|w| content.weapon(w)) {
            Some(w) => {
                let base = w.damage + (mig * w.might_pct + fin * w.finesse_pct) / 100;
                (base * w.light.mult_pct / 100, base * w.heavy.mult_pct / 100)
            }
            None => (b.unarmed_damage, b.unarmed_damage * 3 / 2),
        };
        let spell_power = [&self.equipment.spell1, &self.equipment.spell2]
            .into_iter()
            .flatten()
            .filter_map(|s| content.item(s).and_then(|i| i.spell.as_ref()))
            .map(|s| s.damage + occ * s.occult_pct / 100)
            .max()
            .unwrap_or(0);
        DerivedStats {
            hp_max: b.hp + vit * b.hp_per_vit,
            stamina_max: b.stamina + end * b.stamina_per_end,
            wick_max: b.wick + occ * b.wick_per_occ,
            defense,
            light_damage: light,
            heavy_damage: heavy,
            spell_power,
        }
    }

    pub fn poise(&self, content: &Content) -> u32 {
        let armor = self
            .equipment
            .armor
            .as_deref()
            .and_then(|a| content.item(a))
            .and_then(|i| i.armor.as_ref())
            .map_or(0, |a| a.poise);
        20 + armor
    }

    pub fn add_item(&mut self, content: &Content, item: &str, qty: u32) -> u32 {
        let cap = content.item(item).and_then(|i| i.max_stack).unwrap_or(u32::MAX);
        let slot = self.inventory.entry(item.to_string()).or_insert(0);
        let before = *slot;
        *slot = slot.saturating_add(qty).min(cap);
        let gained = *slot - before;
        if *slot == 0 {
            self.inventory.remove(item);
        }
        if gained > 0 && content.item(item).is_some_and(|i| i.kind == ItemKind::Key) {
            self.flags.insert(format!("got:{item}"));
        }
        gained
    }

    /// Removes up to `qty`; returns false and changes nothing when the
    /// character holds fewer than requested.
    pub fn take_item(&mut self, item: &str, qty: u32) -> bool {
        let Some(have) = self.inventory.get_mut(item) else {
            return false;
        };
        if *have < qty {
            return false;
        }
        *have -= qty;
        if *have == 0 {
            self.inventory.remove(item);
            for slot in [
                &mut self.equipment.main,
                &mut self.equipment.off,
                &mut self.equipment.armor,
                &mut self.equipment.spell1,
                &mut self.equipment.spell2,
            ] {
                if slot.as_deref() == Some(item) {
                    *slot = None;
                }
            }
        }
        true
    }

    pub fn sheet(&self, content: &Content) -> CharacterSheet {
        let inventory = self
            .inventory
            .iter()
            .filter_map(|(id, qty)| {
                let def = content.item(id)?;
                Some(ItemStack {
                    item: id.clone(),
                    name: def.name.clone(),
                    qty: *qty,
                    kind: def.kind.label().to_string(),
                    desc: def.desc.clone(),
                })
            })
            .collect();
        let open_doors = self.flags.iter().filter_map(|f| f.strip_prefix("door:").map(str::to_string)).collect();
        let quests = content
            .quests
            .iter()
            .filter(|q| self.flags.contains(&q.start))
            .map(|q| {
                let note = q
                    .stages
                    .iter()
                    .rev()
                    .find(|s| s.when.iter().all(|f| self.flags.contains(f)))
                    .map(|s| s.note.clone())
                    .unwrap_or_default();
                QuestView { id: q.id.clone(), title: q.title.clone(), note, done: self.flags.contains(&q.done) }
            })
            .collect();
        CharacterSheet {
            id: self.id.clone(),
            name: self.name.clone(),
            archetype: self.archetype,
            look: self.look,
            level: self.level(content),
            attrs: self.attrs,
            echoes: self.echoes,
            next_level_cost: self.next_level_cost(content),
            inventory,
            equipment: self.equipment.clone(),
            open_doors,
            quests,
            checkpoint: self.checkpoint.clone(),
            lanterns: self.lanterns.iter().cloned().collect(),
            stats: self.derived(content),
        }
    }
}

pub fn attr_index(a: Attribute) -> usize {
    match a {
        Attribute::Vitality => 0,
        Attribute::Endurance => 1,
        Attribute::Might => 2,
        Attribute::Finesse => 3,
        Attribute::Occult => 4,
    }
}

/// Echoes needed to go from `level` to `level + 1`.
pub fn level_cost(content: &Content, level: u32) -> u64 {
    let c = &content.balance.level;
    let mut cost = c.base_cost * 1000;
    for _ in 1..level {
        cost = cost * (100 + c.growth_pct) / 100;
    }
    cost / 1000 + c.per_level * level as u64
}
