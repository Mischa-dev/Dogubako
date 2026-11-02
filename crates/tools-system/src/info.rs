//! sysinfo-based tools.

use crate::util::{
    bytes, cpu_features, finish, human_duration, on_path, pct, read_trim, rfc3339, round1, sys_err,
    table,
};
use dogubako_core::prelude::*;
use std::time::Duration;
use sysinfo::{
    Components, CpuRefreshKind, Disks, Gpus, MemoryRefreshKind, Motherboard, Networks,
    ProcessRefreshKind, ProcessesToUpdate, Product, RefreshKind, System, UpdateKind, Users,
};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        overview(),
        cpu(),
        memory(),
        disks(),
        hardware(),
        os(),
        network_interfaces(),
        processes(),
        uptime(),
        temperatures(),
        diagnostics(),
    ]
}

const SAMPLE: Duration = Duration::from_millis(300);

fn spec(id: &str, name: &str, desc: &str, kw: &[&str]) -> ToolSpec {
    ToolSpec::new(id, name, Category::System)
        .describe(desc)
        .keywords(kw)
        .permission(Permission::SystemInfo)
        .timeout(30)
}

/// System with CPU usage sampled over ~300 ms.
fn sampled_cpu_system() -> ToolResult<System> {
    let mut s =
        System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()))
            .map_err(sys_err("CPU information"))?;
    std::thread::sleep(SAMPLE.max(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL));
    s.refresh_cpu_specifics(CpuRefreshKind::everything());
    Ok(s)
}

fn memory_system() -> ToolResult<System> {
    System::new_with_specifics(RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()))
        .map_err(sys_err("memory information"))
}

fn load_avg() -> Value {
    match System::load_average() {
        Ok(l) => {
            json!({ "one": round2(l.one), "five": round2(l.five), "fifteen": round2(l.fifteen) })
        }
        Err(_) => Value::Null,
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn uptime_value() -> Value {
    let up = System::uptime().ok();
    let boot = System::boot_time().ok();
    json!({
        "uptime_seconds": up,
        "uptime_human": up.map(human_duration),
        "boot_time": boot.and_then(rfc3339),
    })
}

fn memory_value(s: &System) -> Value {
    let total = s.total_memory();
    let used = s.used_memory();
    let avail = s.available_memory();
    let mut v = json!({
        "total": bytes(total),
        "used": bytes(used),
        "available": bytes(avail),
        "free": bytes(s.free_memory()),
        "used_percent": pct(used, total),
        "swap_total": bytes(s.total_swap()),
        "swap_used": bytes(s.used_swap()),
        "swap_used_percent": pct(s.used_swap(), s.total_swap()),
    });
    if let Some(c) = s.cgroup_limits() {
        v["cgroup_limit"] = json!({ "total": bytes(c.total_memory), "free": bytes(c.free_memory), "rss": bytes(c.rss), "free_swap": bytes(c.free_swap) });
    }
    v
}

fn disk_kind(k: sysinfo::DiskKind) -> &'static str {
    match k {
        sysinfo::DiskKind::SSD => "SSD",
        sysinfo::DiskKind::HDD => "HDD",
        sysinfo::DiskKind::Unknown(_) => "unknown",
    }
}

fn disks_value() -> ToolResult<Vec<Value>> {
    let d = Disks::new_with_refreshed_list().map_err(sys_err("disk information"))?;
    let mut v: Vec<Value> = d
        .iter()
        .map(|d| {
            let total = d.total_space();
            let avail = d.available_space();
            let used = total.saturating_sub(avail);
            json!({
                "name": d.name().to_string_lossy(),
                "mount_point": d.mount_point().to_string_lossy(),
                "file_system": d.file_system().to_string_lossy(),
                "kind": disk_kind(d.kind()),
                "removable": d.is_removable(),
                "read_only": d.is_read_only(),
                "total": bytes(total),
                "available": bytes(avail),
                "used": bytes(used),
                "used_percent": pct(used, total),
            })
        })
        .collect();
    v.sort_by(|a, b| {
        b["total"]["bytes"]
            .as_u64()
            .cmp(&a["total"]["bytes"].as_u64())
    });
    Ok(v)
}

fn cpu_brand(s: &System) -> (Option<String>, Option<String>) {
    let c = s.cpus().first();
    (
        c.map(|c| c.brand().trim().to_string())
            .filter(|b| !b.is_empty()),
        c.map(|c| c.vendor_id().to_string())
            .filter(|b| !b.is_empty()),
    )
}

// ---------------------------------------------------------------------------

fn overview() -> Arc<dyn Tool> {
    let spec = spec("system.overview", "System Overview", "One-glance dashboard: OS, host, kernel, uptime, CPU usage, memory, swap, load average and disks", &["system", "overview", "dashboard", "cpu", "memory", "ram", "uptime", "disk", "host"]).ui("system");
    sync_tool(spec, |ctx, _p| {
        let mut s = sampled_cpu_system()?;
        s.refresh_memory();
        let (brand, vendor) = cpu_brand(&s);
        let disks = disks_value().unwrap_or_default();
        let mem = memory_value(&s);
        let cpu_usage = round1(s.global_cpu_usage() as f64);
        let data = json!({
            "os": { "name": System::name().ok(), "version": System::os_version().ok(), "long_version": System::long_os_version().ok(), "kernel": System::kernel_version().ok(), "arch": System::cpu_arch() },
            "hostname": System::host_name().ok(),
            "uptime": uptime_value(),
            "cpu": {
                "brand": brand, "vendor": vendor,
                "logical_cores": s.cpus().len(),
                "physical_cores": System::physical_core_count().ok(),
                "usage_percent": cpu_usage,
                "frequency_mhz": s.cpus().first().map(|c| c.frequency()).filter(|f| *f > 0),
            },
            "load_average": load_avg(),
            "memory": mem,
            "disks": disks.iter().take(8).cloned().collect::<Vec<_>>(),
            "process_count": Value::Null,
        });
        let text = format!(
            "{} {} — {} — up {}\nCPU: {} ({} cores) {cpu_usage}% used\nMemory: {} / {} ({}%)",
            data["os"]["name"].as_str().unwrap_or("?"),
            data["os"]["version"].as_str().unwrap_or(""),
            data["hostname"].as_str().unwrap_or("?"),
            data["uptime"]["uptime_human"].as_str().unwrap_or("?"),
            brand.as_deref().unwrap_or("unknown CPU"),
            s.cpus().len(),
            data["memory"]["used"]["human"].as_str().unwrap_or("?"),
            data["memory"]["total"]["human"].as_str().unwrap_or("?"),
            data["memory"]["used_percent"],
        );
        let mut data = data;
        if let Value::Object(m) = &mut data {
            m.remove("process_count");
        }
        Ok(finish(ctx, data, text))
    })
}

fn cpu() -> Arc<dyn Tool> {
    let spec = spec("system.cpu", "CPU Info", "CPU brand, vendor, physical/logical cores, per-core usage (sampled ~300 ms), frequencies and instruction-set features", &["cpu", "processor", "cores", "threads", "usage", "frequency", "avx", "neon"]).ui("system");
    sync_tool(spec, |ctx, _p| {
        let s = sampled_cpu_system()?;
        let (brand, vendor) = cpu_brand(&s);
        let rows: Vec<Vec<Value>> = s
            .cpus()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                vec![
                    json!(i),
                    json!(c.name()),
                    json!(round1(c.usage() as f64)),
                    json!(if c.frequency() > 0 {
                        Some(c.frequency())
                    } else {
                        None
                    }),
                ]
            })
            .collect();
        let freqs: Vec<u64> = s
            .cpus()
            .iter()
            .map(|c| c.frequency())
            .filter(|f| *f > 0)
            .collect();
        let data = json!({
            "brand": brand,
            "vendor": vendor,
            "arch": System::cpu_arch(),
            "logical_cores": s.cpus().len(),
            "physical_cores": System::physical_core_count().ok(),
            "usage_percent": round1(s.global_cpu_usage() as f64),
            "frequency_mhz": { "min": freqs.iter().min(), "max": freqs.iter().max() },
            "features": cpu_features(),
            "load_average": load_avg(),
            "table": table(&["#", "core", "usage %", "MHz"], rows),
        });
        let text = format!(
            "{} — {} logical / {} physical cores — {}% used",
            brand.as_deref().unwrap_or("unknown CPU"),
            s.cpus().len(),
            data["physical_cores"],
            data["usage_percent"]
        );
        Ok(finish(ctx, data, text))
    })
}

fn memory() -> Arc<dyn Tool> {
    let spec = spec(
        "system.memory",
        "Memory Usage",
        "RAM and swap: total, used, available, free and container (cgroup) limits",
        &["memory", "ram", "swap", "usage", "free", "cgroup"],
    )
    .ui("system");
    sync_tool(spec, |ctx, _p| {
        let s = memory_system()?;
        let d = memory_value(&s);
        let text = format!(
            "RAM: {} used of {} ({}%), {} available\nSwap: {} used of {}",
            d["used"]["human"].as_str().unwrap_or(""),
            d["total"]["human"].as_str().unwrap_or(""),
            d["used_percent"],
            d["available"]["human"].as_str().unwrap_or(""),
            d["swap_used"]["human"].as_str().unwrap_or(""),
            d["swap_total"]["human"].as_str().unwrap_or("")
        );
        Ok(finish(ctx, d, text))
    })
}

fn disks() -> Arc<dyn Tool> {
    let spec = spec(
        "system.disks",
        "Disks & Storage",
        "Mounted disks with file system, SSD/HDD kind, capacity, free space and usage",
        &[
            "disk",
            "storage",
            "drive",
            "free space",
            "ssd",
            "hdd",
            "mount",
            "partition",
            "df",
        ],
    )
    .ui("system");
    sync_tool(spec, |ctx, _p| {
        let d = disks_value()?;
        let rows: Vec<Vec<Value>> = d
            .iter()
            .map(|x| {
                vec![
                    x["mount_point"].clone(),
                    x["name"].clone(),
                    x["file_system"].clone(),
                    x["kind"].clone(),
                    x["total"]["human"].clone(),
                    x["used"]["human"].clone(),
                    x["available"]["human"].clone(),
                    x["used_percent"].clone(),
                    x["removable"].clone(),
                ]
            })
            .collect();
        let total: u64 = d.iter().filter_map(|x| x["total"]["bytes"].as_u64()).sum();
        let avail: u64 = d
            .iter()
            .filter_map(|x| x["available"]["bytes"].as_u64())
            .sum();
        let text = if d.is_empty() {
            "No disks reported.".to_string()
        } else {
            format!(
                "{} disk(s), {} free of {}",
                d.len(),
                human_size(avail),
                human_size(total)
            )
        };
        Ok(finish(
            ctx,
            json!({ "count": d.len(), "disks": d, "table": table(&["mount", "device", "fs", "kind", "total", "used", "free", "used %", "removable"], rows) }),
            text,
        ))
    })
}

fn dmi(field: &str) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    read_trim(&format!("/sys/class/dmi/id/{field}")).filter(|v| {
        !matches!(
            v.to_ascii_lowercase().as_str(),
            "to be filled by o.e.m."
                | "default string"
                | "not specified"
                | "none"
                | "system product name"
                | "system manufacturer"
        )
    })
}

fn chassis_name(code: &str) -> Option<&'static str> {
    Some(match code.parse::<u32>().ok()? {
        3 => "Desktop",
        4 => "Low profile desktop",
        6 => "Mini tower",
        7 => "Tower",
        8 => "Portable",
        9 => "Laptop",
        10 => "Notebook",
        11 => "Hand held",
        13 => "All in one",
        14 => "Sub notebook",
        17 => "Main server chassis",
        23 => "Rack mount chassis",
        30 => "Tablet",
        31 => "Convertible",
        32 => "Detachable",
        35 => "Mini PC",
        36 => "Stick PC",
        1 => "Other",
        2 => "Unknown",
        _ => return None,
    })
}

fn gpus_value() -> Vec<Value> {
    match Gpus::new_with_refreshed_list() {
        Ok(g) => g
            .iter()
            .map(|g| json!({ "vendor": g.vendor(), "model": g.model(), "pci": g.pci().to_string(), "total_memory": g.total_memory().map(bytes) }))
            .collect(),
        Err(_) => vec![],
    }
}

fn hardware() -> Arc<dyn Tool> {
    let spec = spec("system.hardware", "Hardware Info", "Computer model, vendor, motherboard, BIOS/firmware, CPU, memory and GPUs (serial numbers are omitted for privacy)", &["hardware", "model", "motherboard", "bios", "vendor", "gpu", "specs", "manufacturer"]);
    sync_tool(spec, |ctx, _p| {
        let s = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing())
                .with_memory(MemoryRefreshKind::nothing().with_ram()),
        )
        .map_err(sys_err("hardware information"))?;
        let (brand, vendor) = cpu_brand(&s);
        let mb = Motherboard::new().ok();
        let product = json!({
            "name": Product::name().ok().filter(|s| !s.is_empty()),
            "family": Product::family().ok().filter(|s| !s.is_empty()),
            "vendor": Product::vendor_name().ok().filter(|s| !s.is_empty()),
            "version": Product::version().ok().filter(|s| !s.is_empty()),
            "sku": Product::stock_keeping_unit().ok().filter(|s| !s.is_empty()),
        });
        let board = json!({
            "name": mb.as_ref().and_then(|m| m.name()),
            "vendor": mb.as_ref().and_then(|m| m.vendor_name()),
            "version": mb.as_ref().and_then(|m| m.version()),
        });
        let firmware = json!({
            "vendor": dmi("bios_vendor"),
            "version": dmi("bios_version"),
            "date": dmi("bios_date"),
            "chassis": dmi("chassis_type").and_then(|c| chassis_name(&c).map(|s| s.to_string())),
            "uefi": if cfg!(target_os = "linux") { json!(std::path::Path::new("/sys/firmware/efi").exists()) } else { Value::Null },
        });
        let gpus = gpus_value();
        let data = json!({
            "product": product,
            "motherboard": board,
            "firmware": firmware,
            "cpu": { "brand": brand, "vendor": vendor, "arch": System::cpu_arch(), "logical_cores": s.cpus().len(), "physical_cores": System::physical_core_count().ok() },
            "memory_total": bytes(s.total_memory()),
            "gpus": gpus,
            "virtualization_hint": dmi("sys_vendor").filter(|v| ["QEMU", "VMware, Inc.", "innotek GmbH", "Xen", "Microsoft Corporation", "Amazon EC2", "Google"].iter().any(|k| v.contains(k))),
        });
        let text = format!(
            "{} {} — {} — {} RAM — {} GPU(s)",
            data["product"]["vendor"].as_str().unwrap_or(""),
            data["product"]["name"].as_str().unwrap_or("unknown model"),
            brand.as_deref().unwrap_or("unknown CPU"),
            human_size(s.total_memory()),
            data["gpus"].as_array().map(|a| a.len()).unwrap_or(0)
        );
        Ok(finish(ctx, data, text.trim().to_string()))
    })
}

fn os() -> Arc<dyn Tool> {
    let spec = spec(
        "system.os",
        "Operating System",
        "OS name, version, kernel, distribution, hostname, architecture, locale and time zone",
        &[
            "os",
            "operating system",
            "version",
            "kernel",
            "distro",
            "locale",
            "timezone",
            "hostname",
        ],
    );
    sync_tool(spec, |ctx, _p| {
        let now = chrono::Local::now();
        let data = json!({
            "name": System::name().ok(),
            "version": System::os_version().ok(),
            "long_version": System::long_os_version().ok(),
            "kernel": System::kernel_version().ok(),
            "kernel_long": System::kernel_long_version().ok(),
            "distribution_id": System::distribution_id(),
            "distribution_like": System::distribution_id_like(),
            "hostname": System::host_name().ok(),
            "arch": System::cpu_arch(),
            "family": std::env::consts::FAMILY,
            "target_os": std::env::consts::OS,
            "locale": sys_locale::get_locale(),
            "locales": sys_locale::get_locales().take(10).collect::<Vec<_>>(),
            "timezone": iana_time_zone::get_timezone().ok(),
            "utc_offset": now.format("%:z").to_string(),
            "local_time": now.to_rfc3339(),
            "open_files_limit": System::open_files_limit().ok(),
            "uptime": uptime_value(),
        });
        let text = format!(
            "{} (kernel {}) on {} — {} — locale {} — tz {}",
            data["long_version"]
                .as_str()
                .or(data["name"].as_str())
                .unwrap_or("?"),
            data["kernel"].as_str().unwrap_or("?"),
            data["arch"].as_str().unwrap_or("?"),
            data["hostname"].as_str().unwrap_or("?"),
            data["locale"].as_str().unwrap_or("?"),
            data["timezone"].as_str().unwrap_or("?")
        );
        Ok(finish(ctx, data, text))
    })
}

fn network_interfaces() -> Arc<dyn Tool> {
    let spec = spec(
        "system.network_interfaces",
        "Network Interfaces",
        "Network adapters with MAC address, IP addresses, MTU, state and traffic counters",
        &[
            "network",
            "interface",
            "adapter",
            "mac",
            "ip address",
            "ifconfig",
            "ipconfig",
            "traffic",
        ],
    )
    .param(
        ParamSpec::boolean("include_loopback")
            .label("Include loopback")
            .default_bool(false),
    );
    sync_tool(spec, |ctx, p| {
        let n = Networks::new_with_refreshed_list().map_err(sys_err("network interfaces"))?;
        let mut list: Vec<(String, &sysinfo::NetworkData)> = n
            .iter()
            .map(|(k, v)| (k.to_string_lossy().to_string(), v))
            .collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        let include_lo = p.bool("include_loopback");
        let mut items = vec![];
        let mut rows = vec![];
        for (name, d) in list {
            let ips: Vec<String> = d
                .ip_networks()
                .iter()
                .map(|ip| format!("{}/{}", ip.addr, ip.prefix))
                .collect();
            let loopback = d.ip_networks().iter().any(|ip| ip.addr.is_loopback()) || name == "lo";
            if loopback && !include_lo {
                continue;
            }
            let mac = d.mac_address();
            let mac_s = if mac.is_unspecified() {
                None
            } else {
                Some(mac.to_string())
            };
            items.push(json!({
                "name": name,
                "mac": mac_s,
                "ips": ips,
                "mtu": d.mtu(),
                "state": format!("{:?}", d.operational_state()).to_lowercase(),
                "loopback": loopback,
                "received": bytes(d.total_received()),
                "transmitted": bytes(d.total_transmitted()),
                "packets_received": d.total_packets_received(),
                "packets_transmitted": d.total_packets_transmitted(),
                "errors_received": d.total_errors_on_received(),
                "errors_transmitted": d.total_errors_on_transmitted(),
                "link_speed_mbps": d.receive_link_speed().or(d.transmit_link_speed()).map(|s| s / 1_000_000),
            }));
            rows.push(vec![
                json!(name),
                json!(mac_s),
                json!(ips.join(", ")),
                json!(d.mtu()),
                json!(format!("{:?}", d.operational_state()).to_lowercase()),
                json!(human_size(d.total_received())),
                json!(human_size(d.total_transmitted())),
            ]);
        }
        let text = items
            .iter()
            .map(|i| {
                format!(
                    "{}: {}",
                    i["name"].as_str().unwrap_or(""),
                    i["ips"]
                        .as_array()
                        .map(|a| a
                            .iter()
                            .filter_map(|x| x.as_str())
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(finish(
            ctx,
            json!({ "count": items.len(), "interfaces": items, "table": table(&["interface", "MAC", "addresses", "MTU", "state", "received", "sent"], rows) }),
            if text.is_empty() {
                "No interfaces reported.".into()
            } else {
                text
            },
        ))
    })
}

fn processes() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("system.processes", "Top Processes", Category::System)
        .describe("Top running processes by CPU or memory with PID, user, status and start time (command lines only on request)")
        .keywords(&["process", "task manager", "top", "htop", "cpu hog", "memory usage", "running programs", "pid"])
        .param(ParamSpec::choice("sort_by", &[("cpu", "CPU usage"), ("memory", "Memory"), ("name", "Name"), ("pid", "PID"), ("start", "Newest first")]).default_str("cpu"))
        .param(ParamSpec::integer("limit").label("Number of processes").range(1.0, 500.0).default_int(25))
        .param(ParamSpec::text("filter").label("Name contains"))
        .param(ParamSpec::boolean("include_command").label("Include full command lines (may contain secrets)").advanced())
        .permission(Permission::ProcessInfo)
        .ui("system")
        .timeout(30);
    sync_tool(spec, |ctx, p| {
        let include_cmd = p.bool("include_command");
        let mut kind = ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_user(UpdateKind::OnlyIfNotSet);
        if include_cmd {
            kind = kind.with_cmd(UpdateKind::OnlyIfNotSet);
        }
        let mut s = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
        )
        .map_err(sys_err("process list"))?;
        s.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        std::thread::sleep(SAMPLE.max(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL));
        s.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        let users = Users::new_with_refreshed_list().ok();
        let filter = p.opt_str("filter").map(|f| f.to_lowercase());
        let total_mem = s.total_memory();
        let mut procs: Vec<&sysinfo::Process> = s
            .processes()
            .values()
            .filter(|pr| pr.thread_kind().is_none())
            .filter(|pr| {
                filter
                    .as_ref()
                    .map(|f| pr.name().to_string_lossy().to_lowercase().contains(f))
                    .unwrap_or(true)
            })
            .collect();
        let count = procs.len();
        match p.str_or("sort_by", "cpu") {
            "memory" => procs.sort_by_key(|p| std::cmp::Reverse(p.memory())),
            "name" => procs.sort_by(|a, b| {
                a.name()
                    .to_ascii_lowercase()
                    .cmp(&b.name().to_ascii_lowercase())
            }),
            "pid" => procs.sort_by_key(|p| p.pid().as_u32()),
            "start" => procs.sort_by_key(|p| std::cmp::Reverse(p.start_time())),
            _ => procs.sort_by(|a, b| {
                b.cpu_usage()
                    .partial_cmp(&a.cpu_usage())
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
        }
        let limit = p.int_or("limit", 25) as usize;
        let mut rows = vec![];
        let mut items = vec![];
        for pr in procs.into_iter().take(limit) {
            let user = pr.user_id().and_then(|uid| {
                users
                    .as_ref()
                    .and_then(|u| u.get_user_by_id(uid))
                    .map(|u| u.name().to_string())
            });
            let cpu = round1(pr.cpu_usage() as f64);
            let mut item = json!({
                "pid": pr.pid().as_u32(),
                "parent_pid": pr.parent().map(|p| p.as_u32()),
                "name": pr.name().to_string_lossy(),
                "cpu_percent": cpu,
                "memory": bytes(pr.memory()),
                "memory_percent": pct(pr.memory(), total_mem),
                "virtual_memory": bytes(pr.virtual_memory()),
                "status": pr.status().to_string(),
                "user": user,
                "start_time": rfc3339(pr.start_time()),
                "run_time_seconds": pr.run_time(),
            });
            if include_cmd {
                item["command"] = json!(pr
                    .cmd()
                    .iter()
                    .map(|c| c.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(" "));
            }
            rows.push(vec![
                item["pid"].clone(),
                item["name"].clone(),
                json!(cpu),
                item["memory"]["human"].clone(),
                item["status"].clone(),
                item["user"].clone(),
                item["start_time"].clone(),
            ]);
            items.push(item);
        }
        let mut cols = vec![
            "pid", "name", "cpu %", "memory", "status", "user", "started",
        ];
        if include_cmd {
            cols.push("command");
            for (r, i) in rows.iter_mut().zip(&items) {
                r.push(i["command"].clone());
            }
        }
        let text = format!("{count} processes; showing {}", items.len());
        Ok(finish(
            ctx,
            json!({ "process_count": count, "processes": items, "table": table(&cols, rows) }),
            text,
        ))
    })
}

fn uptime() -> Arc<dyn Tool> {
    let spec = spec(
        "system.uptime",
        "Uptime",
        "How long the system has been running, boot time and load average",
        &["uptime", "boot time", "load average", "running since"],
    )
    .ui("system");
    sync_tool(spec, |ctx, _p| {
        let mut d = uptime_value();
        if d["uptime_seconds"].is_null() {
            return Err(ToolError::unsupported(
                "uptime is not available on this platform",
            ));
        }
        d["load_average"] = load_avg();
        let text = format!(
            "up {} (since {})",
            d["uptime_human"].as_str().unwrap_or("?"),
            d["boot_time"].as_str().unwrap_or("?")
        );
        Ok(finish(ctx, d, text))
    })
}

fn temperatures() -> Arc<dyn Tool> {
    let spec = spec(
        "system.temperatures",
        "Temperature Sensors",
        "Hardware temperature sensors (CPU, GPU, drives…) with current, max and critical values",
        &[
            "temperature",
            "sensors",
            "thermal",
            "cpu temp",
            "overheating",
            "fan",
        ],
    )
    .ui("system");
    sync_tool(spec, |ctx, _p| {
        let c = Components::new_with_refreshed_list().map_err(sys_err("temperature sensors"))?;
        let mut items: Vec<Value> = c
            .iter()
            .map(|c| json!({ "label": c.label(), "id": c.id(), "temperature_c": c.temperature().filter(|t| t.is_finite()).map(|t| round1(t as f64)), "max_c": c.max().filter(|t| t.is_finite()).map(|t| round1(t as f64)), "critical_c": c.critical().filter(|t| t.is_finite()).map(|t| round1(t as f64)) }))
            .collect();
        items.sort_by(|a, b| a["label"].as_str().cmp(&b["label"].as_str()));
        let rows: Vec<Vec<Value>> = items
            .iter()
            .map(|i| {
                vec![
                    i["label"].clone(),
                    i["temperature_c"].clone(),
                    i["max_c"].clone(),
                    i["critical_c"].clone(),
                ]
            })
            .collect();
        let text = if items.is_empty() {
            "No temperature sensors are exposed on this system.".to_string()
        } else {
            items
                .iter()
                .map(|i| {
                    format!(
                        "{}: {}",
                        i["label"].as_str().unwrap_or(""),
                        i["temperature_c"]
                            .as_f64()
                            .map(|t| format!("{t} °C"))
                            .unwrap_or_else(|| "n/a".into())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let data = json!({ "count": items.len(), "sensors": items, "message": if rows.is_empty() { json!("no temperature sensors exposed (common in VMs and containers)") } else { Value::Null }, "table": table(&["sensor", "°C", "max °C", "critical °C"], rows) });
        Ok(finish(ctx, data, text))
    })
}

fn diagnostics() -> Arc<dyn Tool> {
    let spec = spec("system.diagnostics", "Environment Diagnostics", "Check Dogubako's environment: external programs and versions, temp space, models folder, CPU features and GPU availability", &["diagnostics", "dependencies", "ffmpeg", "doctor", "environment", "gpu", "avx2", "troubleshoot"]).timeout(60);
    sync_tool(spec, |ctx, _p| {
        let deps = ctx.deps().all();
        let dep_rows: Vec<Vec<Value>> = deps
            .iter()
            .map(|d| {
                vec![
                    json!(d.id),
                    json!(d.name),
                    json!(d.found),
                    json!(d.version),
                    json!(d.path.as_ref().map(|p| p.to_string_lossy().to_string())),
                    json!(if d.found {
                        String::new()
                    } else {
                        d.install_hint.clone()
                    }),
                ]
            })
            .collect();
        let found = deps.iter().filter(|d| d.found).count();

        // Temp dir: actually write and read back a file.
        let probe = ctx.temp_path("diag-probe.txt");
        let writable = std::fs::write(&probe, b"dogubako")
            .and_then(|_| std::fs::read(&probe))
            .map(|b| b == b"dogubako")
            .unwrap_or(false);
        let _ = std::fs::remove_file(&probe);
        let work = ctx.work_dir().to_path_buf();
        let free = Disks::new_with_refreshed_list().ok().and_then(|d| {
            let canon = std::fs::canonicalize(&work).unwrap_or(work.clone());
            d.iter()
                .filter(|x| canon.starts_with(x.mount_point()))
                .max_by_key(|x| x.mount_point().as_os_str().len())
                .map(|x| {
                    (
                        x.mount_point().to_string_lossy().to_string(),
                        x.available_space(),
                    )
                })
        });
        let models_dir = ctx.models_dir().to_path_buf();
        let installed: Vec<Value> = ctx
            .models()
            .list()
            .into_iter()
            .filter(|m| m.installed)
            .map(|m| json!(m.spec.id))
            .collect();

        let mut dri: Vec<String> = vec![];
        if let Ok(rd) = std::fs::read_dir("/dev/dri") {
            dri = rd
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
                .collect();
            dri.sort();
        }
        let vulkan = [
            "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
            "/usr/lib/aarch64-linux-gnu/libvulkan.so.1",
            "/usr/lib/libvulkan.so.1",
            "/usr/lib64/libvulkan.so.1",
        ]
        .iter()
        .any(|p| std::path::Path::new(p).exists());
        let gpu = json!({
            "devices": gpus_value(),
            "dev_dri": dri,
            "nvidia_device": std::path::Path::new("/dev/nvidia0").exists(),
            "nvidia_smi": on_path("nvidia-smi").map(|p| p.to_string_lossy().to_string()),
            "vulkan_loader_hint": vulkan,
            "metal": cfg!(target_os = "macos"),
        });
        let data = json!({
            "dogubako_version": env!("CARGO_PKG_VERSION"),
            "target": format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            "available_parallelism": std::thread::available_parallelism().map(|n| n.get()).ok(),
            "dependencies_found": found,
            "dependencies_total": deps.len(),
            "temp": {
                "dir": work.to_string_lossy(),
                "writable": writable,
                "filesystem": free.as_ref().map(|f| f.0.clone()),
                "free": free.map(|f| bytes(f.1)),
            },
            "models": { "dir": models_dir.to_string_lossy(), "exists": models_dir.is_dir(), "installed": installed },
            "cpu_features": cpu_features(),
            "gpu": gpu,
            "table": table(&["id", "program", "found", "version", "path", "install hint"], dep_rows),
        });
        let mut out = finish(
            ctx,
            data,
            format!(
                "{found}/{} external programs found; temp dir {}",
                deps.len(),
                if writable { "writable" } else { "NOT writable" }
            ),
        );
        if !writable {
            out = out.warn("temporary directory is not writable");
        }
        Ok(out)
    })
}
