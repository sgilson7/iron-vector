//! The campaign: five planets, each a fixed map with its own missions, and
//! the player's progress through them. Which planets and missions are open,
//! which parts are unlocked, and where each sits in the Hasse diagram are
//! all decided here.

use crate::content::Rgb;
use crate::map::{Climate, MapSpec};
use crate::mission::MissionSpec;
use crate::parts::Catalog;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// How a planet looks: the scene's colours, its fog and its light.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    pub sky: Rgb,
    pub fog: Rgb,
    pub ground: Rgb,
    pub grid: Rgb,
    pub buildings: Vec<Rgb>,
    pub roof: Rgb,
    pub window: Rgb,
    /// lit window bands on the buildings
    pub windows: bool,
    pub light_dir: [i32; 3],
    pub fog_near_m: i32,
    pub fog_far_m: i32,
    /// how much the ground glows, in percent (lava)
    #[serde(default)]
    pub ground_glow_pct: i32,
    /// a field of stars behind the fog
    #[serde(default)]
    pub stars: bool,
    /// the colour of buildings a mission asks you to protect
    pub structure: Rgb,
    /// how far the view reaches, and how near it starts; giants need both larger
    #[serde(default = "default_far")]
    pub far_m: i32,
    #[serde(default = "default_near")]
    pub near_cm: i32,
}

fn default_far() -> i32 {
    3200
}

fn default_near() -> i32 {
    12
}

/// How a planet is drawn on the star map.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Star {
    pub color: Rgb,
    pub band: Rgb,
    /// percent of the standard globe
    pub size: i32,
    pub rings: bool,
    pub moons: i32,
    /// 1 for a planet that glows of itself
    pub glow: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Planet {
    pub id: String,
    pub name: String,
    pub opens_at: usize,
    /// also needs this many plus challenges met
    #[serde(default)]
    pub opens_at_plus: usize,
    /// left off the star map until it opens
    #[serde(default)]
    pub hidden: bool,
    pub blurb: String,
    pub gimmick: String,
    pub star: Star,
    pub look: Look,
    pub climate: Climate,
    pub map: MapSpec,
    pub missions: Vec<MissionSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Campaign {
    #[serde(rename = "_note", default)]
    pub note: String,
    pub planets: Vec<Planet>,
}

impl Campaign {
    /// Reads and checks the campaign against the catalogue: every reward is a
    /// real part, no part is given twice, no starter part is given at all, a
    /// requirement names an earlier mission on the same planet, and planets
    /// open in order.
    pub fn parse(json: &str, cat: &Catalog) -> Result<Campaign, String> {
        let c: Campaign = serde_json::from_str(json).map_err(|e| format!("planets: {e}"))?;
        let starters = starters(cat);
        let mut given = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut last_open = 0;
        for p in &c.planets {
            if p.opens_at < last_open {
                return Err(format!("planets: {} opens before the planet ahead of it", p.id));
            }
            last_open = p.opens_at;
            if p.missions.is_empty() {
                return Err(format!("planets: {} has no missions", p.id));
            }
            for (i, m) in p.missions.iter().enumerate() {
                if m.kind.staged() && m.stages.is_empty() {
                    return Err(format!("planets: {} is an assault with no stages", m.id));
                }
                if !ids.insert(m.id.clone()) {
                    return Err(format!("planets: duplicate mission id {}", m.id));
                }
                for r in &m.requires {
                    if !p.missions[..i].iter().any(|e| &e.id == r) {
                        return Err(format!(
                            "planets: {} requires {r}, not an earlier mission on {}",
                            m.id, p.id
                        ));
                    }
                }
                for part in [&m.reward, &m.plus.reward] {
                    if cat.get(part).is_none() {
                        return Err(format!("planets: {} rewards unknown part {part}", m.id));
                    }
                    if starters.contains(part) {
                        return Err(format!(
                            "planets: {} rewards {part}, which every player starts with",
                            m.id
                        ));
                    }
                    if !given.insert(part.clone()) {
                        return Err(format!("planets: {part} is given by more than one mission"));
                    }
                }
            }
        }
        Ok(c)
    }

    /// The planet and the mission with this id.
    pub fn find(&self, id: &str) -> Option<(usize, usize)> {
        self.planets
            .iter()
            .enumerate()
            .find_map(|(p, pl)| pl.missions.iter().position(|m| m.id == id).map(|i| (p, i)))
    }

    pub fn mission(&self, id: &str) -> Option<&MissionSpec> {
        self.find(id).map(|(p, i)| &self.planets[p].missions[i])
    }

    /// A mission's level in the Hasse diagram: the length of its longest
    /// chain of requirements.
    pub fn level(&self, planet: usize, i: usize) -> usize {
        let p = &self.planets[planet];
        p.missions[i]
            .requires
            .iter()
            .filter_map(|r| p.missions.iter().position(|m| &m.id == r))
            .map(|j| self.level(planet, j) + 1)
            .max()
            .unwrap_or(0)
    }

    pub fn total_missions(&self) -> usize {
        self.planets.iter().map(|p| p.missions.len()).sum()
    }
}

/// The parts every player owns from the start: the default loadout.
pub fn starters(cat: &Catalog) -> BTreeSet<String> {
    cat.default_loadout
        .values()
        .chain(cat.starters.iter())
        .cloned()
        .collect()
}

/// What the player has done.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Progress {
    pub cleared: BTreeSet<String>,
    #[serde(default)]
    pub plus: BTreeSet<String>,
}

/// Where a mission stands for the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    Locked,
    Open,
    Cleared,
    Plus,
}

impl Progress {
    /// Keeps only what this campaign knows: a mission id that no longer
    /// exists is dropped, and a plus counts only on a cleared mission.
    pub fn clean(mut self, c: &Campaign) -> Progress {
        self.cleared.retain(|id| c.find(id).is_some());
        let cleared = self.cleared.clone();
        self.plus.retain(|id| cleared.contains(id));
        self
    }

    pub fn planet_open(&self, c: &Campaign, p: usize) -> bool {
        let pl = &c.planets[p];
        self.cleared.len() >= pl.opens_at && self.plus.len() >= pl.opens_at_plus
    }

    /// Whether the star map shows a planet at all.
    pub fn planet_shown(&self, c: &Campaign, p: usize) -> bool {
        !c.planets[p].hidden || self.planet_open(c, p)
    }

    pub fn standing(&self, c: &Campaign, id: &str) -> Standing {
        let Some((p, i)) = c.find(id) else {
            return Standing::Locked;
        };
        if self.plus.contains(id) {
            return Standing::Plus;
        }
        if self.cleared.contains(id) {
            return Standing::Cleared;
        }
        let reqs_met = c.planets[p].missions[i]
            .requires
            .iter()
            .all(|r| self.cleared.contains(r));
        if self.planet_open(c, p) && reqs_met {
            Standing::Open
        } else {
            Standing::Locked
        }
    }

    pub fn can_fly(&self, c: &Campaign, id: &str) -> bool {
        self.standing(c, id) != Standing::Locked
    }

    /// Every part the player may fit.
    pub fn unlocked(&self, c: &Campaign, cat: &Catalog) -> BTreeSet<String> {
        let mut out = starters(cat);
        for p in &c.planets {
            for m in &p.missions {
                if self.cleared.contains(&m.id) {
                    out.insert(m.reward.clone());
                }
                if self.plus.contains(&m.id) {
                    out.insert(m.plus.reward.clone());
                }
            }
        }
        out
    }

    /// Which mission gives a part, and whether by its plus challenge.
    pub fn source<'a>(c: &'a Campaign, part: &str) -> Option<(&'a MissionSpec, bool)> {
        c.planets.iter().flat_map(|p| &p.missions).find_map(|m| {
            if m.reward == part {
                Some((m, false))
            } else if m.plus.reward == part {
                Some((m, true))
            } else {
                None
            }
        })
    }

    /// Records a result; returns the parts it unlocked, each with whether it
    /// came from the plus challenge.
    pub fn record(&mut self, c: &Campaign, id: &str, success: bool, plus_met: bool) -> Vec<(String, bool)> {
        let Some(m) = c.mission(id) else { return Vec::new() };
        let mut new = Vec::new();
        if success && self.cleared.insert(id.to_string()) {
            new.push((m.reward.clone(), false));
        }
        // the plus challenge is hidden until the first clear, so it can only
        // be met on a run after that one
        let revealed_before = success && !new.is_empty();
        if success && plus_met && !revealed_before && self.plus.insert(id.to_string()) {
            new.push((m.plus.reward.clone(), true));
        }
        new
    }
}

/// Planet counts in one place, for the page's progress line.
pub fn counts(c: &Campaign, p: &Progress) -> BTreeMap<&'static str, usize> {
    BTreeMap::from([
        ("cleared", p.cleared.len()),
        ("plus", p.plus.len()),
        ("total", c.total_missions()),
    ])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::parts::tests::catalog;

    pub fn campaign() -> Campaign {
        Campaign::parse(include_str!("../../../data/planets.json"), &catalog()).unwrap()
    }

    fn cleared(ids: &[&str]) -> Progress {
        Progress {
            cleared: ids.iter().map(|s| s.to_string()).collect(),
            plus: BTreeSet::new(),
        }
    }

    #[test]
    fn five_planets_open_at_two_five_eight_and_eleven_and_a_hidden_sixth_wants_plus_clears() {
        let c = campaign();
        assert!(c.planets.iter().all(|p| (4..=8).contains(&p.missions.len())));
        let shown: Vec<usize> = c
            .planets
            .iter()
            .filter(|p| !p.hidden)
            .map(|p| p.opens_at)
            .collect();
        assert_eq!(shown, vec![0, 2, 5, 8, 11]);
        let hidden: Vec<&Planet> = c.planets.iter().filter(|p| p.hidden).collect();
        assert_eq!(hidden.len(), 1);
        assert!(hidden[0].opens_at_plus > 0);
    }

    #[test]
    fn the_hidden_planet_stays_off_the_map_until_enough_plus_challenges_are_met() {
        let c = campaign();
        let t = c.planets.iter().position(|p| p.hidden).unwrap();
        let need = c.planets[t].opens_at_plus;
        let all: Vec<String> = c
            .planets
            .iter()
            .filter(|p| !p.hidden)
            .flat_map(|p| p.missions.iter().map(|m| m.id.clone()))
            .collect();
        let mut p = Progress {
            cleared: all.iter().cloned().collect(),
            plus: BTreeSet::new(),
        };
        assert!(
            !p.planet_shown(&c, t),
            "every mission cleared, no plus: still hidden"
        );
        p.plus = all.iter().take(need - 1).cloned().collect();
        assert!(!p.planet_shown(&c, t));
        p.plus = all.iter().take(need).cloned().collect();
        assert!(p.planet_shown(&c, t) && p.planet_open(&c, t));
    }

    #[test]
    fn the_second_planet_opens_after_two_missions() {
        let c = campaign();
        assert!(!cleared(&["halden-1"]).planet_open(&c, 1));
        assert!(cleared(&["halden-1", "halden-2"]).planet_open(&c, 1));
        assert_eq!(cleared(&["halden-1"]).standing(&c, "sere-1"), Standing::Locked);
        assert_eq!(
            cleared(&["halden-1", "halden-2"]).standing(&c, "sere-1"),
            Standing::Open
        );
    }

    #[test]
    fn a_mission_waits_for_its_requirements() {
        let c = campaign();
        let p = Progress::default();
        assert_eq!(p.standing(&c, "halden-1"), Standing::Open);
        assert_eq!(p.standing(&c, "halden-3"), Standing::Locked, "needs halden-1");
        assert_eq!(cleared(&["halden-1"]).standing(&c, "halden-3"), Standing::Open);
    }

    #[test]
    fn hasse_levels_count_the_longest_chain() {
        let c = campaign();
        let (p, i) = c.find("sere-4").unwrap();
        // sere-4 needs sere-2 and sere-3; sere-3 needs sere-1
        assert_eq!(c.level(p, i), 2);
        let (p, i) = c.find("halden-1").unwrap();
        assert_eq!(c.level(p, i), 0);
    }

    #[test]
    fn a_clear_gives_the_reward_and_reveals_the_plus_which_a_later_run_can_win() {
        let c = campaign();
        let mut p = Progress::default();
        let first = p.record(&c, "halden-1", true, true);
        assert_eq!(
            first,
            vec![("hd-kestrel".to_string(), false)],
            "the plus is hidden on the first clear"
        );
        let again = p.record(&c, "halden-1", true, true);
        assert_eq!(again, vec![("cr-vane".to_string(), true)]);
        assert!(p.record(&c, "halden-1", true, true).is_empty(), "nothing twice");
        let parts = p.unlocked(&c, &catalog());
        assert!(parts.contains("hd-kestrel") && parts.contains("cr-vane") && parts.contains("hd-warden"));
        assert!(!parts.contains("lg-wisp"));
    }

    #[test]
    fn a_failure_gives_nothing() {
        let c = campaign();
        let mut p = Progress::default();
        assert!(p.record(&c, "halden-1", false, true).is_empty());
        assert!(p.cleared.is_empty());
    }

    #[test]
    fn a_save_naming_an_unknown_mission_or_an_uncleared_plus_is_cleaned() {
        let c = campaign();
        let p: Progress =
            serde_json::from_str(r#"{"cleared":["halden-1","gone-9"],"plus":["halden-1","halden-2"]}"#)
                .unwrap();
        let p = p.clean(&c);
        assert_eq!(p.cleared, BTreeSet::from(["halden-1".to_string()]));
        assert_eq!(p.plus, BTreeSet::from(["halden-1".to_string()]));
    }

    #[test]
    fn every_part_but_the_starters_is_a_reward_exactly_once() {
        let c = campaign();
        let cat = catalog();
        let all: BTreeSet<String> = cat.parts.iter().map(|p| p.id.clone()).collect();
        let everything = Progress {
            cleared: c
                .planets
                .iter()
                .flat_map(|p| p.missions.iter().map(|m| m.id.clone()))
                .collect(),
            plus: BTreeSet::new(),
        };
        let everything = Progress {
            plus: everything.cleared.clone(),
            ..everything
        };
        assert_eq!(everything.unlocked(&c, &cat), all);
    }

    #[test]
    fn a_campaign_that_gives_a_part_twice_is_refused() {
        let text = include_str!("../../../data/planets.json").replacen(
            "\"reward\": \"cr-vane\"",
            "\"reward\": \"hd-kestrel\"",
            1,
        );
        assert!(Campaign::parse(&text, &catalog())
            .unwrap_err()
            .contains("more than one"));
    }
}
