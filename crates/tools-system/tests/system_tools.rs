use dogubako_core::engine::{Engine, EngineConfig, RunRequest};
use dogubako_core::params::LocalFileResolver;
use dogubako_core::policy::PermissionSet;
use dogubako_core::registry::Registry;
use dogubako_core::spec::Permission;
use dogubako_core::testing::TestHarness;
use dogubako_core::tool::Environment;
use serde_json::{json, Value};

fn harness() -> TestHarness {
    TestHarness::new(dogubako_tools_system::tools())
}

#[tokio::test]
async fn overview_memory_cpu() {
    let h = harness();
    let out = h.run("system.overview", json!({})).await.unwrap();
    let d = &out.data;
    assert!(d["memory"]["total"]["bytes"].as_u64().unwrap() > 0);
    assert!(d["cpu"]["logical_cores"].as_u64().unwrap() >= 1);
    let usage = d["cpu"]["usage_percent"].as_f64().unwrap();
    assert!((0.0..=100.0).contains(&usage));
    assert!(d["uptime"]["uptime_seconds"].as_u64().unwrap() > 0);
    assert!(d["os"]["arch"].as_str().unwrap().len() > 1);
    assert!(d.get("host_note").is_none(), "CLI env is not hosted");

    let out = h.run("system.memory", json!({})).await.unwrap();
    let total = out.data["total"]["bytes"].as_u64().unwrap();
    assert!(total > 0);
    assert!(out.data["used"]["bytes"].as_u64().unwrap() <= total);
    assert!(out.data["available"]["bytes"].as_u64().unwrap() <= total);

    let out = h.run("system.cpu", json!({})).await.unwrap();
    let rows = out.data["table"]["rows"].as_array().unwrap();
    assert_eq!(
        rows.len() as u64,
        out.data["logical_cores"].as_u64().unwrap()
    );
    for r in rows {
        let u = r[2].as_f64().unwrap();
        assert!((0.0..=100.0).contains(&u));
    }
    #[cfg(target_arch = "x86_64")]
    assert!(out.data["features"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == "sse2"));
}

#[tokio::test]
async fn disks_os_uptime_net() {
    let h = harness();
    let out = h.run("system.disks", json!({})).await.unwrap();
    for d in out.data["disks"].as_array().unwrap() {
        let total = d["total"]["bytes"].as_u64().unwrap();
        assert!(d["available"]["bytes"].as_u64().unwrap() <= total);
        assert!(["SSD", "HDD", "unknown"].contains(&d["kind"].as_str().unwrap()));
    }
    let out = h.run("system.os", json!({})).await.unwrap();
    assert!(out.data["target_os"].as_str().unwrap().len() > 1);
    assert!(out.data["utc_offset"].as_str().unwrap().contains(':'));
    let out = h.run("system.uptime", json!({})).await.unwrap();
    assert!(out.data["uptime_seconds"].as_u64().unwrap() > 0);
    assert!(out.data["boot_time"].as_str().is_some());
    let out = h
        .run(
            "system.network_interfaces",
            json!({"include_loopback": true}),
        )
        .await
        .unwrap();
    let ifs = out.data["interfaces"].as_array().unwrap();
    if cfg!(target_os = "linux") {
        assert!(
            ifs.iter().any(|i| i["loopback"] == true),
            "loopback expected with include_loopback"
        );
    }
    let out = h.run("system.network_interfaces", json!({})).await.unwrap();
    assert!(out.data["interfaces"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["loopback"] == false));
}

#[tokio::test]
async fn battery_hardware_temperatures() {
    let h = harness();
    // The CI container has no battery: the tool must succeed with an empty list.
    let out = h.run("system.battery", json!({})).await.unwrap();
    let n = out.data["count"].as_u64().unwrap();
    assert_eq!(n as usize, out.data["batteries"].as_array().unwrap().len());
    if n == 0 {
        assert_eq!(out.data["detected"], false);
        assert_eq!(out.data["message"], "no battery detected");
    } else {
        for b in out.data["batteries"].as_array().unwrap() {
            if let Some(c) = b["charge_percent"].as_f64() {
                assert!((0.0..=100.0).contains(&c));
            }
        }
    }
    let out = h.run("system.hardware", json!({})).await.unwrap();
    assert!(out.data["memory_total"]["bytes"].as_u64().unwrap() > 0);
    let s = serde_json::to_string(&out.data).unwrap();
    assert!(!s.contains("serial"), "serial numbers must not be reported");
    let out = h.run("system.temperatures", json!({})).await.unwrap();
    assert_eq!(
        out.data["count"].as_u64().unwrap() as usize,
        out.data["sensors"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn processes_and_privacy() {
    let h = harness();
    let out = h
        .run("system.processes", json!({"limit": 5, "sort_by": "memory"}))
        .await
        .unwrap();
    let procs = out.data["processes"].as_array().unwrap();
    assert!(!procs.is_empty() && procs.len() <= 5);
    assert!(
        procs.iter().all(|p| p.get("command").is_none()),
        "command lines are opt-in"
    );
    let mems: Vec<u64> = procs
        .iter()
        .map(|p| p["memory"]["bytes"].as_u64().unwrap())
        .collect();
    assert!(mems.windows(2).all(|w| w[0] >= w[1]), "sorted by memory");
    // Our own test process must be findable by PID.
    let me = std::process::id();
    let out = h
        .run(
            "system.processes",
            json!({"limit": 500, "sort_by": "pid", "include_command": true}),
        )
        .await
        .unwrap();
    let mine = out.data["processes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pid"] == me);
    if let Some(p) = mine {
        assert!(p["command"].as_str().is_some());
    }
    let out = h
        .run(
            "system.processes",
            json!({"filter": "zzzz-no-such-process-zzzz"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["process_count"], 0);
    assert_eq!(
        h.run("system.processes", json!({"limit": 0}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );
}

#[tokio::test]
async fn diagnostics_reports_deps() {
    let h = harness();
    let out = h.run("system.diagnostics", json!({})).await.unwrap();
    let d = &out.data;
    assert_eq!(d["temp"]["writable"], true);
    assert!(d["dependencies_total"].as_u64().unwrap() >= 10);
    let rows = d["table"]["rows"].as_array().unwrap();
    // ffmpeg is installed in this environment; if found, a version is reported.
    if let Some(ff) = rows.iter().find(|r| r[0] == "ffmpeg" && r[2] == true) {
        assert!(ff[3].as_str().is_some());
    }
    assert!(d["available_parallelism"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn hosted_note_and_permissions() {
    let mut reg = Registry::new();
    reg.register_all(dogubako_tools_system::tools());
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = EngineConfig::local(Environment::Server);
    cfg.temp_root = dir.path().join("tmp");
    let engine = Engine::new(reg, cfg, vec![]);
    let run = |id: &'static str, perms: PermissionSet| {
        let engine = &engine;
        let out_dir = dir.path().join(format!("out-{id}"));
        async move {
            let resolver = LocalFileResolver::unrestricted();
            engine
                .run(RunRequest {
                    tool_id: id,
                    params: Default::default(),
                    resolver: &resolver,
                    out_dir,
                    permissions: perms,
                    authorize: None,
                    progress: None,
                    cancel: Default::default(),
                    timeout: None,
                    fs: None,
                })
                .await
                .map(|(o, _)| o)
        }
    };
    let out = run("system.memory", PermissionSet::all()).await.unwrap();
    assert!(out.data["host_note"].as_str().unwrap().contains("server"));
    assert_eq!(out.data["environment"], "server");
    assert!(!out.warnings.is_empty());
    let err = run(
        "system.processes",
        PermissionSet::from_iter([Permission::SystemInfo]),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "permission_denied");
    let err = run("system.overview", PermissionSet::none())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "permission_denied");
}

#[test]
fn specs() {
    for t in dogubako_tools_system::tools() {
        let s = t.spec();
        assert!(s.id.starts_with("system."));
        assert!(
            !s.permissions.is_empty(),
            "{} must declare a permission",
            s.id
        );
        let _: Value = serde_json::to_value(s).unwrap();
    }
}
