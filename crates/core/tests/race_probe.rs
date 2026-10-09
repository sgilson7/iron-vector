//! A tuning aid, not a check: flies each race with the plain racer pilot in
//! the player's starting frame and prints when each racer passed each gate.
//! Run with `RACES=halden-2 cargo test -p iv_core --test race_probe -- --ignored --nocapture`.

use iv_core::campaign::Campaign;
use iv_core::content::Palette;
use iv_core::fx;
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
    let ids = std::env::var("RACES").unwrap_or_default();
    for id in ids.split(',').filter(|s| !s.is_empty()) {
        let (p, i) = camp.find(id).unwrap();
        let spec = &camp.planets[p].missions[i];
        let mut w = World::mission(
            &cat,
            &cat.default_loadout(),
            pal.scheme(0).paints(),
            &camp.planets[p],
            spec,
            &pilots,
            &units,
            pal.enemy.paints(),
            pal.giant.paints(),
        );
        let mut bot = pilots.pilot("racer", 9).unwrap();
        let m0 = &w.mechs[0];
        println!(
            "chest {} m, height {} m, half width {} m",
            (m0.chest().y - m0.body.pos.y) >> 16,
            m0.tuning.height >> 16,
            m0.tuning.half_width >> 16
        );
        let n = w.mission.as_ref().unwrap().course.len();
        let mut seen = vec![vec![None; n]; w.mechs.len()];
        let mut knocks = vec![0; w.mechs.len()];
        for t in 0..240 * 60u32 {
            let d = bot.think(&w.senses(0));
            w.mechs[0].body.aim_yaw = d.aim_yaw & (fx::TURN - 1);
            w.mechs[0].body.aim_pitch = d
                .aim_pitch
                .clamp(iv_core::mech::PITCH_MIN, iv_core::mech::PITCH_MAX);
            let before: Vec<i32> = w.mechs.iter().map(|m| m.stagger).collect();
            w.tick(d.controls);
            for (k, m) in w.mechs.iter().enumerate() {
                if m.stagger == iv_core::course::KNOCK_TICKS && before[k] == 0 {
                    knocks[k] += 1;
                }
                for g in seen[k].iter_mut().take(m.course_next.min(n)) {
                    g.get_or_insert(t);
                }
            }
            if std::env::var("TRACE").is_ok_and(|v| v == id) && t % 30 == 0 {
                for (k, m) in w.mechs.iter().enumerate() {
                    let p = m.body.pos;
                    println!(
                        "    t {:5.1} #{k} gate {:2} pos ({:5}, {:3}, {:5}) en {:3}% {} {}",
                        t as f32 / 60.0,
                        m.course_next,
                        p.x >> 16,
                        p.y >> 16,
                        p.z >> 16,
                        m.body.en_pct(&m.tuning),
                        if m.body.grounded { "ground" } else { "air" },
                        if k == 0 {
                            bot.current_move().map_or("", |(n, _)| n)
                        } else {
                            m.intent.as_deref().unwrap_or("")
                        }
                    );
                }
            }
            let m = w.mission.as_mut().unwrap();
            m.ended_at = None;
            m.success = None;
            if w.mechs.iter().all(|m| m.course_next >= n) {
                break;
            }
        }
        println!("{id}: plus {} s, limit {} s", spec.plus.value, spec.time_limit_s);
        for (k, m) in w.mechs.iter().enumerate() {
            let times: Vec<String> = seen[k]
                .iter()
                .map(|t| t.map_or("--".into(), |t| format!("{:.1}", t as f32 / 60.0)))
                .collect();
            let p = m.body.pos;
            println!(
                "  {:<18} knocks {} at gate {} pos ({}, {}, {}): {}",
                if k == 0 { "PLAYER (bot)" } else { &m.name },
                knocks[k],
                m.course_next,
                p.x >> 16,
                p.y >> 16,
                p.z >> 16,
                times.join(" ")
            );
        }
    }
}
