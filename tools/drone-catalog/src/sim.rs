// Runs every drone through DarkFenX's refine engine with an all-V pilot and
// reads back its modified attributes and damage.

use std::collections::HashMap;
use std::path::Path;

use refine_rs::{
    AddFitCmd, AddSolCmd, CmdResp, FitAddDroneCmd, FitAddSkillCmd, FitSetShipCmd, ItemInfoMode,
    ItemTypeId, MinionState, Refine, SkillLevel,
    stats::{GetItemStatsCmd, StatOption, StatOptionExt},
};
use serde_json::Value;

// A hull without drone bonuses, so only skills apply.
const APOCALYPSE: i32 = 642;

pub struct Simulated {
    // Modified attribute values by attribute id
    pub attrs: HashMap<i64, f64>,
    // EM, thermal, kinetic, explosive
    pub dps: [f64; 4],
}

pub async fn simulate(
    data_dir: &Path,
    cache_dir: &Path,
    skills: &[i64],
    drones: &[i64],
) -> HashMap<i64, Simulated> {
    std::fs::create_dir_all(cache_dir).expect("cache dir");
    let refine = Refine::with_fs_adc(2, 2, cache_dir.to_path_buf());
    refine
        .add_src_with_phb_fs("tq", true, data_dir.to_path_buf())
        .await
        .expect("refine could not load the dump");

    let mut out = HashMap::new();
    for &drone in drones {
        let mut sol = refine
            .add_sol(None, AddSolCmd::default())
            .await
            .expect("solar system");
        let item_id = {
            let mut fit = sol.add_fit(AddFitCmd::new()).await.expect("fit");
            let mut cmds: Vec<refine_rs::ChangeFitEnumCmd> =
                vec![FitSetShipCmd::new(ItemTypeId::from_i32(APOCALYPSE)).into()];
            for &skill in skills {
                let level = SkillLevel::from_i32_checked(5).expect("skill level");
                cmds.push(FitAddSkillCmd::new(ItemTypeId::from_i32(skill as i32), level).into());
            }
            cmds.push(
                FitAddDroneCmd::new(ItemTypeId::from_i32(drone as i32), MinionState::Engaging)
                    .into(),
            );
            let resps = fit.change(cmds).await.expect("fit change");
            resps
                .get(skills.len() + 1)
                .and_then(CmdResp::get_item_id)
                .expect("drone item id")
        };

        let mut item = sol.get_item(item_id).await.expect("drone item");
        let info = serde_json::to_value(item.get_info(ItemInfoMode::Full).await).expect("info");
        let cmd = GetItemStatsCmd {
            default: false,
            dmg: StatOptionExt::Enabled,
            speed: StatOption::Enabled,
            ..Default::default()
        };
        let stats = serde_json::to_value(item.get_stats(cmd).await).expect("stats");
        let _ = sol.remove().await;

        // Attributes serialize as {"e<id>": [base, modified]}
        let attrs = info["attrs"]
            .as_object()
            .map(|attrs| {
                attrs
                    .iter()
                    .filter_map(|(key, vals)| {
                        let id = key.strip_prefix('e')?.parse().ok()?;
                        Some((id, vals.get(1)?.as_f64()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Damage serializes as [[dps], [volley]], each [em, therm, kin, exp, ...]
        let dps_entry = &stats["dmg"][0][0];
        let component = |i: usize| dps_entry.get(i).and_then(Value::as_f64).unwrap_or(0.0);
        let dps = [component(0), component(1), component(2), component(3)];
        out.insert(drone, Simulated { attrs, dps });
    }
    out
}
