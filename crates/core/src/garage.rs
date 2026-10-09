//! The garage: choose a slot, look at the parts that fit it, compare one
//! against what is fitted, fit it, and pick a paint scheme. Everything the
//! garage screen shows is built here; the page lays it out.

use crate::campaign::{Campaign, Progress};
use crate::content::{hex, Palette};
use crate::parts::{stats, Catalog, Loadout, Part, Slot, Stats, WeaponKind};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Garage {
    /// index into `Slot::ALL`
    pub slot: usize,
    /// index into the parts that fit the slot, while the pointer is over one
    pub hover: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlotRow {
    pub key: String,
    pub part: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartRow {
    pub name: String,
    pub maker: String,
    pub equipped: bool,
    pub hovered: bool,
    /// not yet won; `hint` names the mission that gives it, and whether by its plus
    pub locked: bool,
    pub hint: Option<(String, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Detail {
    pub name: String,
    pub maker: String,
    pub blurb: String,
    pub kind: Option<String>,
    /// (copy key, value)
    pub rows: Vec<(String, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatRow {
    pub key: String,
    pub value: i32,
    pub next: i32,
    /// 1 if the candidate is better, −1 if worse, 0 if the same
    pub better: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaintRow {
    pub name: String,
    pub swatch: Vec<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GarageView {
    pub slots: Vec<SlotRow>,
    pub parts: Vec<PartRow>,
    pub detail: Detail,
    pub stats: Vec<StatRow>,
    /// copy keys of the candidate frame's problems
    pub warnings: Vec<String>,
    pub paints: Vec<PaintRow>,
}

/// A stat the garage shows: its copy key, whether higher is better, and how to read it.
type Shown = (&'static str, bool, fn(&Stats) -> i32);

/// Which way is better for each frame stat shown, and how to read it.
const SHOWN: &[Shown] = &[
    ("stat_ap", true, |s| s.ap),
    ("stat_weight", false, |s| s.weight),
    ("stat_load_limit", true, |s| s.load_limit),
    ("stat_arms_weight", false, |s| s.arms_weight),
    ("stat_arms_limit", true, |s| s.arms_limit),
    ("stat_en_load", false, |s| s.en_load),
    ("stat_en_output", true, |s| s.en_output),
    ("stat_en_capacity", true, |s| s.en_capacity),
    ("stat_en_recovery", true, |s| s.en_recovery),
    ("stat_supply_delay_ms", false, |s| s.supply_delay_ms),
    ("stat_walk_kmh", true, |s| s.walk_kmh * s.mobility_pct / 100),
    ("stat_glide_kmh", true, |s| s.glide_kmh * s.mobility_pct / 100),
    ("stat_qb_kmh", true, |s| s.qb_kmh * s.mobility_pct / 100),
    ("stat_qb_en", false, |s| s.qb_en),
    ("stat_ascend_ms2", true, |s| s.ascend_ms2 * s.mobility_pct / 100),
    ("stat_ascend_en", false, |s| s.ascend_en),
    ("stat_lock_range_m", true, |s| s.lock_range_m),
    ("stat_stability", true, |s| s.stability),
];

fn compare(now: &Stats, next: &Stats) -> Vec<StatRow> {
    SHOWN
        .iter()
        .map(|(key, higher_is_better, read)| {
            let (a, b) = (read(now), read(next));
            let up = (b > a) as i32 - (b < a) as i32;
            StatRow {
                key: key.to_string(),
                value: a,
                next: b,
                better: if *higher_is_better { up } else { -up },
            }
        })
        .collect()
}

fn warnings(s: &Stats) -> Vec<String> {
    [
        (s.overweight, "warn_overweight"),
        (s.over_en, "warn_over_en"),
        (s.over_arms, "warn_over_arms"),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, k)| k.to_string())
    .collect()
}

fn kind_key(k: WeaponKind) -> String {
    let name = match k {
        WeaponKind::Rifle => "rifle",
        WeaponKind::Shotgun => "shotgun",
        WeaponKind::Missile => "missile",
        WeaponKind::Laser => "laser",
        WeaponKind::Grenade => "grenade",
        WeaponKind::Melee => "melee",
        WeaponKind::Plasma => "plasma",
    };
    format!("kind_{name}")
}

/// The numbers a part's card shows, skipping those that do not apply.
pub fn detail_rows(p: &Part) -> Vec<(String, i32)> {
    let s = p.stats;
    let rate = if s.fire_ms > 0 { 60_000 / s.fire_ms } else { 0 };
    let all: [(&str, i32); 25] = [
        ("stat_damage", s.damage),
        ("stat_impact", s.impact),
        ("stat_rate", rate),
        ("stat_salvo", if s.salvo > 1 { s.salvo } else { 0 }),
        ("stat_speed_ms", s.speed_ms),
        ("stat_range_m", s.range_m),
        ("stat_homing_dps", s.homing_dps),
        ("stat_blast_m", s.blast_m),
        ("stat_ammo", s.ammo),
        ("stat_ap", s.ap),
        ("stat_weight", s.weight),
        ("stat_en_load", s.en_load),
        ("stat_stability", s.stability),
        ("stat_load_limit", s.load_limit),
        ("stat_walk_kmh", s.walk_kmh),
        ("stat_jump_ms", s.jump_ms),
        ("stat_arms_limit", s.arms_limit),
        ("stat_lock_range_m", s.lock_range_m),
        ("stat_glide_kmh", s.glide_kmh),
        ("stat_qb_kmh", s.qb_kmh),
        ("stat_qb_en", s.qb_en),
        ("stat_ascend_ms2", s.ascend_ms2),
        ("stat_en_capacity", s.en_capacity),
        ("stat_en_recovery", s.en_recovery),
        ("stat_en_output", s.en_output),
    ];
    let melee = p.kind == Some(WeaponKind::Melee);
    all.iter()
        .filter(|(_, v)| *v != 0)
        // a melee weapon's numbers mean a lunge and a reach, and it has no rounds
        .filter(|(k, _)| !(melee && *k == "stat_ammo"))
        .map(|(k, v)| {
            let k = match (melee, *k) {
                (true, "stat_range_m") => "stat_lunge_m",
                (true, "stat_speed_ms") => "stat_lunge_ms",
                (true, "stat_blast_m") => "stat_reach_m",
                (_, k) => k,
            };
            (k.to_string(), *v)
        })
        .collect()
}

impl Garage {
    pub fn slot(&self) -> Slot {
        Slot::ALL[self.slot.min(Slot::ALL.len() - 1)]
    }

    /// The loadout as it would be with the hovered part fitted.
    pub fn candidate(&self, l: &Loadout, cat: &Catalog) -> Loadout {
        let mut c = l.clone();
        if let Some(p) = self.hover.and_then(|i| cat.for_slot(self.slot()).get(i).copied()) {
            c.0.insert(self.slot(), p.id.clone());
        }
        c
    }

    pub fn view(
        &self,
        l: &Loadout,
        paint: usize,
        cat: &Catalog,
        pal: &Palette,
        unlocked: &BTreeSet<String>,
        camp: &Campaign,
    ) -> GarageView {
        let slot = self.slot();
        let choices = cat.for_slot(slot);
        let fitted = l.part(slot, cat);
        let shown = self.hover.and_then(|i| choices.get(i).copied()).unwrap_or(fitted);
        let now = stats(l, cat);
        let next = stats(&self.candidate(l, cat), cat);
        GarageView {
            slots: Slot::ALL
                .iter()
                .enumerate()
                .map(|(i, s)| SlotRow {
                    key: format!(
                        "slot_{}",
                        serde_json::to_value(s)
                            .ok()
                            .and_then(|v| v.as_str().map(String::from))
                            .unwrap_or_default()
                    ),
                    part: l.part(*s, cat).name.clone(),
                    selected: i == self.slot,
                })
                .collect(),
            parts: choices
                .iter()
                .enumerate()
                .map(|(i, p)| PartRow {
                    name: p.name.clone(),
                    maker: p.maker.clone(),
                    equipped: p.id == fitted.id,
                    hovered: Some(i) == self.hover,
                    locked: !unlocked.contains(&p.id),
                    hint: (!unlocked.contains(&p.id))
                        .then(|| Progress::source(camp, &p.id).map(|(m, plus)| (m.name.clone(), plus)))
                        .flatten(),
                })
                .collect(),
            detail: Detail {
                name: shown.name.clone(),
                maker: shown.maker.clone(),
                blurb: shown.blurb.clone(),
                kind: shown.kind.map(kind_key),
                rows: detail_rows(shown),
            },
            stats: compare(&now, &next),
            warnings: warnings(&next),
            paints: pal
                .schemes
                .iter()
                .enumerate()
                .map(|(i, s)| PaintRow {
                    name: s.name.clone(),
                    swatch: [s.primary, s.secondary, s.dark, s.glow]
                        .iter()
                        .map(|c| hex(*c))
                        .collect(),
                    selected: i == paint,
                })
                .collect(),
        }
    }

    /// Fits a part from the current slot's list. An index past the list, or
    /// a part not yet won, does nothing.
    pub fn equip(&mut self, i: usize, l: &mut Loadout, cat: &Catalog, unlocked: &BTreeSet<String>) -> bool {
        let Some(p) = cat.for_slot(self.slot()).get(i).copied() else {
            return false;
        };
        if !unlocked.contains(&p.id) {
            return false;
        }
        l.0.insert(self.slot(), p.id.clone());
        true
    }

    pub fn select(&mut self, slot: usize) {
        self.slot = slot.min(Slot::ALL.len() - 1);
        self.hover = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::tests::campaign;
    use crate::content::tests::palette;
    use crate::parts::tests::catalog;

    fn everything(c: &Catalog) -> BTreeSet<String> {
        c.parts.iter().map(|p| p.id.clone()).collect()
    }

    fn slot_index(s: Slot) -> usize {
        Slot::ALL.iter().position(|x| *x == s).unwrap()
    }

    #[test]
    fn hovering_a_lighter_head_shows_less_weight_as_better() {
        let c = catalog();
        let l = c.default_loadout();
        let mut g = Garage::default();
        g.select(slot_index(Slot::Head));
        g.hover = Some(
            c.for_slot(Slot::Head)
                .iter()
                .position(|p| p.id == "hd-kestrel")
                .unwrap(),
        );
        let v = g.view(&l, 0, &c, &palette(), &everything(&c), &campaign());
        let w = v.stats.iter().find(|r| r.key == "stat_weight").unwrap();
        // warden 3000 kg → kestrel 2200 kg
        assert_eq!(w.next - w.value, -800);
        assert_eq!(w.better, 1);
        let lock = v.stats.iter().find(|r| r.key == "stat_lock_range_m").unwrap();
        assert_eq!((lock.value, lock.next, lock.better), (360, 440, 1));
        assert_eq!(
            v.stats.iter().find(|r| r.key == "stat_ap").unwrap().better,
            -1,
            "the light head has less AP"
        );
        assert_eq!(v.detail.name, "HD-07 KESTREL");
    }

    #[test]
    fn heavy_parts_on_light_legs_warn_of_overweight() {
        let c = catalog();
        let mut l = c.default_loadout();
        for (slot, id) in [
            (Slot::Legs, "lg-wisp"),
            (Slot::Core, "cr-citadel"),
            (Slot::Arms, "ar-bastion"),
            (Slot::ShoulderWeapon, "lc-halo"),
        ] {
            l.0.insert(slot, id.to_string());
        }
        let v = Garage::default().view(&l, 0, &c, &palette(), &everything(&c), &campaign());
        assert!(
            v.warnings.contains(&"warn_overweight".to_string()),
            "{:?}",
            v.warnings
        );
    }

    #[test]
    fn a_locked_part_shows_where_it_is_won_and_cannot_be_fitted() {
        let c = catalog();
        let camp = campaign();
        let starters = crate::campaign::starters(&c);
        let mut l = c.default_loadout();
        let mut g = Garage::default();
        g.select(slot_index(Slot::Head));
        let kestrel = c
            .for_slot(Slot::Head)
            .iter()
            .position(|p| p.id == "hd-kestrel")
            .unwrap();
        let v = g.view(&l, 0, &c, &palette(), &starters, &camp);
        let row = &v.parts[kestrel];
        assert!(row.locked);
        assert_eq!(row.hint, Some(("GUNSHIP SWEEP".to_string(), false)));
        assert!(!g.equip(kestrel, &mut l, &c, &starters));
        assert_eq!(l, c.default_loadout());
        assert!(g.equip(kestrel, &mut l, &c, &everything(&c)));
    }

    #[test]
    fn fitting_a_part_changes_only_its_slot() {
        let c = catalog();
        let mut l = c.default_loadout();
        let before = l.clone();
        let mut g = Garage::default();
        g.select(slot_index(Slot::Legs));
        assert!(g.equip(1, &mut l, &c, &everything(&c)));
        let changed: Vec<_> = Slot::ALL.iter().filter(|s| l.0[s] != before.0[s]).collect();
        assert_eq!(changed, vec![&Slot::Legs]);
        assert!(
            !g.equip(99, &mut l, &c, &everything(&c)),
            "an index past the list does nothing"
        );
    }

    #[test]
    fn both_hands_offer_every_arm_weapon() {
        let c = catalog();
        let arm_weapons = c.parts.iter().filter(|p| p.slot == Slot::RightWeapon).count();
        let melee = c
            .parts
            .iter()
            .filter(|p| p.kind == Some(WeaponKind::Melee))
            .count();
        assert_eq!(melee, 2);
        assert_eq!(c.for_slot(Slot::RightWeapon).len(), arm_weapons);
        assert_eq!(
            c.for_slot(Slot::LeftWeapon).len(),
            arm_weapons - melee,
            "no blade in the left hand"
        );
    }

    #[test]
    fn a_weapon_card_shows_its_rate_of_fire_by_hand() {
        let c = catalog();
        let rows = detail_rows(c.get("rf-marrow").unwrap());
        // 120 ms between rounds = 500 a minute
        assert!(rows.contains(&("stat_rate".to_string(), 500)));
        assert!(
            !rows.iter().any(|(k, _)| k == "stat_salvo"),
            "a single-shot weapon shows no salvo"
        );
    }

    #[test]
    fn every_slot_label_has_a_copy_string() {
        let copy: serde_json::Value = serde_json::from_str(include_str!("../../../data/copy.json")).unwrap();
        let c = catalog();
        let v = Garage::default().view(
            &c.default_loadout(),
            0,
            &c,
            &palette(),
            &everything(&c),
            &campaign(),
        );
        for s in &v.slots {
            assert!(copy[&s.key].is_string(), "{} has no copy", s.key);
        }
        for r in &v.stats {
            assert!(copy[&r.key].is_string(), "{} has no copy", r.key);
        }
        for p in &c.parts {
            for (k, _) in detail_rows(p) {
                assert!(copy[&k].is_string(), "{k} has no copy");
            }
            if let Some(k) = p.kind {
                assert!(copy[&kind_key(k)].is_string());
            }
        }
    }
}
