//! `system.battery` via starship-battery.

use crate::util::{finish, human_duration, round1, table};
use dogubako_core::prelude::*;
use starship_battery::units::electric_potential::volt;
use starship_battery::units::energy::watt_hour;
use starship_battery::units::power::watt;
use starship_battery::units::ratio::percent;
use starship_battery::units::thermodynamic_temperature::degree_celsius;
use starship_battery::units::time::second;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![battery()]
}

fn finite(v: f32) -> Option<f64> {
    if v.is_finite() {
        Some(round1(v as f64))
    } else {
        None
    }
}

fn round2(v: f32) -> Option<f64> {
    if v.is_finite() {
        Some((v as f64 * 100.0).round() / 100.0)
    } else {
        None
    }
}

fn describe(i: usize, b: &starship_battery::Battery) -> Value {
    let energy = b.energy().get::<watt_hour>();
    let full = b.energy_full().get::<watt_hour>();
    let design = b.energy_full_design().get::<watt_hour>();
    // Health = current full capacity / design capacity; only when both are real.
    let health = if design > 0.0 && full > 0.0 {
        finite(full / design * 100.0)
    } else {
        None
    };
    let rate = b.energy_rate().get::<watt>();
    let state = format!("{}", b.state());
    json!({
        "index": i,
        "vendor": b.vendor(),
        "model": b.model(),
        "technology": format!("{}", b.technology()),
        "state": state,
        "charge_percent": finite(b.state_of_charge().get::<percent>()),
        "health_percent": health,
        "energy_wh": round2(energy),
        "energy_full_wh": if full > 0.0 { round2(full) } else { None },
        "energy_full_design_wh": if design > 0.0 { round2(design) } else { None },
        "power_w": if rate.is_finite() && rate != 0.0 { round2(rate) } else { None },
        "voltage_v": round2(b.voltage().get::<volt>()).filter(|v| *v > 0.0),
        "temperature_c": b.temperature().and_then(|t| finite(t.get::<degree_celsius>())),
        "cycle_count": b.cycle_count(),
        "time_to_full": b.time_to_full().map(|t| t.get::<second>() as u64).map(|s| json!({ "seconds": s, "human": human_duration(s) })),
        "time_to_empty": b.time_to_empty().map(|t| t.get::<second>() as u64).map(|s| json!({ "seconds": s, "human": human_duration(s) })),
    })
}

fn battery() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("system.battery", "Battery Health", Category::System)
        .describe("Battery charge, state, health (full vs design capacity), cycle count, energy, power draw, voltage, temperature and time remaining")
        .keywords(&["battery", "health", "cycles", "charge", "capacity", "wear", "laptop", "power"])
        .permission(Permission::SystemInfo)
        .ui("system")
        .timeout(30);
    sync_tool(spec, |ctx, _p| {
        let mut batteries = vec![];
        let mut errors = vec![];
        match starship_battery::Manager::new() {
            Ok(m) => match m.batteries() {
                Ok(iter) => {
                    for (i, b) in iter.enumerate() {
                        match b {
                            Ok(b) => batteries.push(describe(i, &b)),
                            Err(e) => errors.push(format!("battery {i}: {e}")),
                        }
                    }
                }
                Err(e) => errors.push(format!("cannot enumerate batteries: {e}")),
            },
            Err(e) => errors.push(format!("battery information unavailable: {e}")),
        }
        let text = if batteries.is_empty() {
            if errors.is_empty() {
                "No battery detected.".to_string()
            } else {
                format!("No battery information available ({})", errors.join("; "))
            }
        } else {
            batteries
                .iter()
                .map(|b| {
                    format!(
                        "Battery {}: {}% {}{}{}",
                        b["index"],
                        b["charge_percent"]
                            .as_f64()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "?".into()),
                        b["state"].as_str().unwrap_or(""),
                        b["health_percent"]
                            .as_f64()
                            .map(|h| format!(", health {h}%"))
                            .unwrap_or_default(),
                        b["cycle_count"]
                            .as_u64()
                            .map(|c| format!(", {c} cycles"))
                            .unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let rows: Vec<Vec<Value>> = batteries
            .iter()
            .map(|b| {
                vec![
                    b["index"].clone(),
                    b["model"].clone(),
                    b["state"].clone(),
                    b["charge_percent"].clone(),
                    b["health_percent"].clone(),
                    b["cycle_count"].clone(),
                    b["energy_full_wh"].clone(),
                    b["energy_full_design_wh"].clone(),
                ]
            })
            .collect();
        let data = json!({
            "count": batteries.len(),
            "detected": !batteries.is_empty(),
            "message": if batteries.is_empty() { json!("no battery detected") } else { Value::Null },
            "batteries": batteries,
            "errors": errors,
            "table": table(&["#", "model", "state", "charge %", "health %", "cycles", "full Wh", "design Wh"], rows),
        });
        Ok(finish(ctx, data, text))
    })
}
