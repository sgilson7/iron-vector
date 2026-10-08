//! Parts, loadouts and the stats a loadout adds up to.
//!
//! Every part is data (`data/parts.json`). Lengths in that file are
//! centimetres, angles degrees, speeds km/h, so the file holds only integers
//! and the core never parses a float.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Head,
    Core,
    Arms,
    Legs,
    Booster,
    Generator,
    RightWeapon,
    LeftWeapon,
    ShoulderWeapon,
}

impl Slot {
    pub const ALL: [Slot; 9] = [
        Slot::RightWeapon,
        Slot::LeftWeapon,
        Slot::ShoulderWeapon,
        Slot::Head,
        Slot::Core,
        Slot::Arms,
        Slot::Legs,
        Slot::Booster,
        Slot::Generator,
    ];

    /// The part category a slot takes: the two arm-weapon slots share theirs.
    pub fn takes(self, part_slot: Slot) -> bool {
        match self {
            Slot::RightWeapon | Slot::LeftWeapon => part_slot == Slot::RightWeapon,
            s => s == part_slot,
        }
    }

    pub fn is_weapon(self) -> bool {
        matches!(
            self,
            Slot::RightWeapon | Slot::LeftWeapon | Slot::ShoulderWeapon
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoxSpec {
    pub mesh: String,
    /// centre, cm, relative to the part's origin
    pub at: [i32; 3],
    /// cm
    pub size: [i32; 3],
    /// pitch, yaw, roll in degrees
    #[serde(default)]
    pub rot: [i32; 3],
    pub paint: String,
    /// "leg_l" or "leg_r" swing with the walk; "aim" follows the aim pitch
    #[serde(default)]
    pub anim: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponKind {
    Rifle,
    Shotgun,
    Missile,
    Laser,
    Grenade,
}

/// Every stat a part can carry. Each slot uses the ones that mean something
/// for it; the rest stay zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PartStats {
    pub ap: i32,
    pub weight: i32,
    pub en_load: i32,
    // legs
    pub load_limit: i32,
    pub walk_kmh: i32,
    pub jump_ms: i32,
    // arms
    pub arms_limit: i32,
    // head
    pub lock_range_m: i32,
    // booster
    pub glide_kmh: i32,
    pub qb_kmh: i32,
    pub qb_en: i32,
    pub ascend_ms2: i32,
    pub ascend_en: i32,
    // generator
    pub en_capacity: i32,
    pub en_recovery: i32,
    pub en_output: i32,
    pub supply_delay_ms: i32,
    // weapons
    pub damage: i32,
    pub impact: i32,
    pub fire_ms: i32,
    pub speed_ms: i32,
    pub ammo: i32,
    pub range_m: i32,
    pub homing_dps: i32,
    pub salvo: i32,
    pub spread_deg: i32,
    pub blast_m: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: String,
    pub slot: Slot,
    pub name: String,
    pub maker: String,
    pub blurb: String,
    #[serde(default)]
    pub kind: Option<WeaponKind>,
    pub stats: PartStats,
    /// Named attachment points, cm from the part's origin.
    #[serde(default)]
    pub mounts: BTreeMap<String, [i32; 3]>,
    #[serde(default)]
    pub boxes: Vec<BoxSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub parts: Vec<Part>,
    /// The loadout a new player starts with, and what an invalid save becomes.
    pub default_loadout: BTreeMap<Slot, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogError(pub String);

/// Mounts each frame part must name, so the model can be assembled.
const REQUIRED_MOUNTS: &[(Slot, &[&str])] = &[
    (Slot::Legs, &["core"]),
    (Slot::Core, &["head", "arm", "back", "booster"]),
    (Slot::Arms, &["hand"]),
];

impl Catalog {
    pub fn parse(json: &str) -> Result<Catalog, CatalogError> {
        let c: Catalog = serde_json::from_str(json).map_err(|e| CatalogError(e.to_string()))?;
        c.validate()?;
        Ok(c)
    }

    fn validate(&self) -> Result<(), CatalogError> {
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.parts {
            if !seen.insert(p.id.as_str()) {
                return Err(CatalogError(format!("duplicate part id {}", p.id)));
            }
            if p.slot == Slot::LeftWeapon {
                return Err(CatalogError(format!(
                    "{}: arm weapons are listed as right_weapon and fit either hand",
                    p.id
                )));
            }
            if p.slot.is_weapon() != p.kind.is_some() {
                return Err(CatalogError(format!(
                    "{}: a weapon needs a kind, and only a weapon",
                    p.id
                )));
            }
            for b in &p.boxes {
                if crate::mesh::Mesh::from_name(&b.mesh).is_none() {
                    return Err(CatalogError(format!("{}: unknown mesh {}", p.id, b.mesh)));
                }
                if b.size.iter().any(|s| *s <= 0) {
                    return Err(CatalogError(format!("{}: a box has no size", p.id)));
                }
            }
            for (slot, names) in REQUIRED_MOUNTS {
                if p.slot == *slot {
                    for n in *names {
                        if !p.mounts.contains_key(*n) {
                            return Err(CatalogError(format!("{}: missing mount {n}", p.id)));
                        }
                    }
                }
            }
        }
        for slot in Slot::ALL {
            let id = self
                .default_loadout
                .get(&slot)
                .ok_or_else(|| CatalogError(format!("default loadout has no {slot:?}")))?;
            let part = self
                .get(id)
                .ok_or_else(|| CatalogError(format!("default loadout names unknown part {id}")))?;
            if !slot.takes(part.slot) {
                return Err(CatalogError(format!("{id} does not fit {slot:?}")));
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.id == id)
    }

    /// The parts that fit a slot, in file order.
    pub fn for_slot(&self, slot: Slot) -> Vec<&Part> {
        self.parts.iter().filter(|p| slot.takes(p.slot)).collect()
    }

    pub fn default_loadout(&self) -> Loadout {
        Loadout(self.default_loadout.clone())
    }
}

/// One part id per slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loadout(pub BTreeMap<Slot, String>);

impl Loadout {
    /// Reads a saved loadout. Any slot that is missing, unknown or holds a
    /// part that does not fit takes the default part instead, so a save from
    /// an older catalogue still loads.
    pub fn restore(saved: &str, cat: &Catalog) -> Loadout {
        let parsed: BTreeMap<Slot, String> = serde_json::from_str(saved).unwrap_or_default();
        let mut l = cat.default_loadout();
        for (slot, id) in parsed {
            if cat.get(&id).map(|p| slot.takes(p.slot)).unwrap_or(false) {
                l.0.insert(slot, id);
            }
        }
        l
    }

    pub fn save(&self) -> String {
        serde_json::to_string(&self.0).unwrap_or_default()
    }

    pub fn part<'a>(&self, slot: Slot, cat: &'a Catalog) -> &'a Part {
        let id = &self.0[&slot];
        cat.get(id).expect("a loadout holds only catalogue ids")
    }
}

/// What a loadout adds up to. These are the numbers the garage shows and the
/// numbers the mech flies with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Stats {
    pub ap: i32,
    pub weight: i32,
    pub load_limit: i32,
    pub arms_weight: i32,
    pub arms_limit: i32,
    pub en_load: i32,
    pub en_output: i32,
    pub en_capacity: i32,
    pub en_recovery: i32,
    pub supply_delay_ms: i32,
    pub walk_kmh: i32,
    pub glide_kmh: i32,
    pub qb_kmh: i32,
    pub qb_en: i32,
    pub ascend_ms2: i32,
    pub ascend_en: i32,
    pub jump_ms: i32,
    pub lock_range_m: i32,
    /// percent of speed kept after the overweight penalty
    pub mobility_pct: i32,
    /// percent of EN recovery kept after the EN-load penalty
    pub recovery_pct: i32,
    pub overweight: bool,
    pub over_en: bool,
    pub over_arms: bool,
}

/// Percent of speed lost per percent of weight over the legs' load limit.
const OVERWEIGHT_PENALTY_PER_PCT: i32 = 2;
/// The slowest an overweight frame can get, in percent.
const MIN_MOBILITY_PCT: i32 = 40;
/// EN recovery kept while EN load exceeds output, in percent.
const OVER_EN_RECOVERY_PCT: i32 = 35;

pub fn stats(l: &Loadout, cat: &Catalog) -> Stats {
    let mut s = Stats::default();
    for slot in Slot::ALL {
        let p = l.part(slot, cat).stats;
        s.ap += p.ap;
        s.en_load += p.en_load;
        if !matches!(slot, Slot::Legs) {
            s.weight += p.weight;
        }
        if matches!(slot, Slot::RightWeapon | Slot::LeftWeapon) {
            s.arms_weight += p.weight;
        }
    }
    let legs = l.part(Slot::Legs, cat).stats;
    let arms = l.part(Slot::Arms, cat).stats;
    let head = l.part(Slot::Head, cat).stats;
    let booster = l.part(Slot::Booster, cat).stats;
    let generator = l.part(Slot::Generator, cat).stats;
    // The legs carry everything above them; their own weight counts toward
    // the total shown but not toward the load they must bear.
    s.load_limit = legs.load_limit;
    let carried = s.weight;
    s.weight += legs.weight;
    s.arms_limit = arms.arms_limit;
    s.en_output = generator.en_output;
    s.en_capacity = generator.en_capacity;
    s.supply_delay_ms = generator.supply_delay_ms;
    s.walk_kmh = legs.walk_kmh;
    s.jump_ms = legs.jump_ms;
    s.glide_kmh = booster.glide_kmh;
    s.qb_kmh = booster.qb_kmh;
    s.qb_en = booster.qb_en;
    s.ascend_ms2 = booster.ascend_ms2;
    s.ascend_en = booster.ascend_en;
    s.lock_range_m = head.lock_range_m;

    s.overweight = carried > s.load_limit;
    s.mobility_pct = if s.overweight {
        let over_pct = (carried - s.load_limit) * 100 / s.load_limit.max(1);
        (100 - over_pct * OVERWEIGHT_PENALTY_PER_PCT).max(MIN_MOBILITY_PCT)
    } else {
        100
    };
    s.over_en = s.en_load > s.en_output;
    s.recovery_pct = if s.over_en { OVER_EN_RECOVERY_PCT } else { 100 };
    s.en_recovery = generator.en_recovery * s.recovery_pct / 100;
    s.over_arms = s.arms_weight > s.arms_limit;
    s
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn catalog() -> Catalog {
        Catalog::parse(include_str!("../../../data/parts.json")).expect("data/parts.json is valid")
    }

    #[test]
    fn the_shipped_catalogue_is_valid_and_every_slot_has_a_part() {
        let c = catalog();
        for slot in Slot::ALL {
            assert!(!c.for_slot(slot).is_empty(), "{slot:?}");
        }
    }

    #[test]
    fn stats_add_up_by_hand() {
        let c = catalog();
        let l = c.default_loadout();
        let s = stats(&l, &c);
        let sum = |f: fn(&PartStats) -> i32| {
            Slot::ALL
                .iter()
                .map(|sl| f(&l.part(*sl, &c).stats))
                .sum::<i32>()
        };
        assert_eq!(s.ap, sum(|p| p.ap));
        assert_eq!(s.weight, sum(|p| p.weight));
        assert_eq!(s.en_load, sum(|p| p.en_load));
    }

    #[test]
    fn overweight_costs_two_percent_of_speed_per_percent_over() {
        let mut c = catalog();
        let l = c.default_loadout();
        let legs_id = l.0[&Slot::Legs].clone();
        let carried = stats(&l, &c).weight - l.part(Slot::Legs, &c).stats.weight;
        // set the legs' limit so the load is exactly 10% over: 90% → 80%
        let legs = c.parts.iter_mut().find(|p| p.id == legs_id).unwrap();
        legs.stats.load_limit = carried * 10 / 11;
        let s = stats(&l, &c);
        assert!(s.overweight);
        let over_pct = (carried - s.load_limit) * 100 / s.load_limit;
        assert_eq!(s.mobility_pct, 100 - 2 * over_pct);
        assert!((78..=82).contains(&s.mobility_pct), "{}", s.mobility_pct);
    }

    #[test]
    fn a_save_with_an_unknown_or_misfit_part_falls_back_per_slot() {
        let c = catalog();
        let d = c.default_loadout();
        let head = c.for_slot(Slot::Head).last().unwrap().id.clone();
        let saved = format!(
            r#"{{"head":"{head}","core":"no-such-part","legs":"{}"}}"#,
            d.0[&Slot::Head]
        );
        let l = Loadout::restore(&saved, &c);
        assert_eq!(l.0[&Slot::Head], head);
        assert_eq!(l.0[&Slot::Core], d.0[&Slot::Core]);
        assert_eq!(
            l.0[&Slot::Legs],
            d.0[&Slot::Legs],
            "a head cannot be fitted as legs"
        );
        assert_eq!(Loadout::restore("not json", &c), d);
    }

    #[test]
    fn a_loadout_survives_a_save_and_restore() {
        let c = catalog();
        let l = c.default_loadout();
        assert_eq!(Loadout::restore(&l.save(), &c), l);
    }

    #[test]
    fn a_catalogue_with_a_duplicate_id_is_refused() {
        let mut c = catalog();
        c.parts.push(c.parts[0].clone());
        assert!(c.validate().is_err());
    }
}
