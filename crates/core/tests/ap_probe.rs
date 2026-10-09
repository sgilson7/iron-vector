//! A tuning aid: holds W in a mission and prints the player's AP every half second.
//! `MISSION=spindle-4 cargo test -p iv_core --test ap_probe -- --ignored --nocapture`
use iv_core::campaign::Campaign;
use iv_core::content::Palette;
use iv_core::mech::Controls;
use iv_core::mission::Units;
use iv_core::parts::Catalog;
use iv_core::pilot::Pilots;
use iv_core::world::World;

#[test]
#[ignore]
fn probe() {
    let cat = Catalog::parse(include_str!("../../../data/parts.json")).unwrap();
    let pal = Palette::parse(include_str!("../../../data/palette.json")).unwrap();
    let camp = Campaign::parse(include_str!("../../../data/planets.json"), &cat).unwrap();
    let pilots = Pilots::parse(include_str!("../../../data/pilots.json")).unwrap();
    let units = Units::parse(include_str!("../../../data/units.json")).unwrap();
    let id = std::env::var("MISSION").unwrap();
    let (p, i) = camp.find(&id).unwrap();
    let mut w = World::mission(
        &cat,
        &cat.default_loadout(),
        pal.scheme(0).paints(),
        &camp.planets[p],
        &camp.planets[p].missions[i],
        &pilots,
        &units,
        pal.enemy.paints(),
        pal.giant.paints(),
    );
    let hold = std::env::var("STILL").is_err();
    for t in 0..12 * 60 {
        let c = Controls {
            move_z: if hold { 1 << 16 } else { 0 },
            ..Controls::default()
        };
        let before = w.mechs[0].ap;
        w.tick(c);
        let lost = before - w.mechs[0].ap;
        if lost > 0 {
            let near: Vec<String> = w
                .mechs
                .iter()
                .skip(1)
                .map(|m| format!("{} {}m", m.name, (m.chest().sub(w.mechs[0].chest()).len() >> 16)))
                .collect();
            println!(
                "t {:5.2} lost {lost:5} ap {:5} shots {} strikes {} | {}",
                t as f32 / 60.0,
                w.mechs[0].ap,
                w.shots.len(),
                w.strikes.len(),
                near.join(", ")
            );
        }
    }
}
