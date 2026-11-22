//! Offline administration of a server data directory (users, agents,
//! tokens) and extension trust management.

use crate::render::Out;
use clap::{Arg, ArgAction, ArgMatches, Command};
use dogubako_core::manifest::{load_extensions, parse_manifest, sha256_hex, TrustStore};
use dogubako_core::spec::Permission;
use dogubako_server::auth;
use dogubako_server::authz::{Policy, Role};
use dogubako_server::db::Db;
use serde_json::json;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

fn data_dir_arg() -> Arg {
    Arg::new("data-dir")
        .long("data-dir")
        .short('d')
        .env("DOGUBAKO_DATA_DIR")
        .default_value("./data")
        .help("Server data directory")
        .global(true)
}

fn policy_args(c: Command) -> Command {
    c.arg(
        Arg::new("allow")
            .long("allow")
            .action(ArgAction::Append)
            .help("Allowed tool patterns, e.g. '*', 'image.*', 'video.compress'"),
    )
    .arg(
        Arg::new("deny")
            .long("deny")
            .action(ArgAction::Append)
            .help("Denied tool patterns"),
    )
    .arg(Arg::new("permissions").long("permissions").help(
        "Comma-separated capabilities: network,host_fs_read,host_fs_write,system_info,process_info",
    ))
    .arg(
        Arg::new("max-jobs")
            .long("max-jobs")
            .value_parser(clap::value_parser!(usize))
            .help("Max concurrent jobs"),
    )
    .arg(
        Arg::new("quota-mb")
            .long("quota-mb")
            .value_parser(clap::value_parser!(u64))
            .help("Storage quota (MiB)"),
    )
    .arg(
        Arg::new("discovery")
            .long("discovery")
            .value_parser(["full", "compact"])
            .help("MCP discovery mode"),
    )
}

pub fn users_cmd() -> Command {
    Command::new("users")
        .about("Manage accounts and agents of a server data directory")
        .subcommand_required(true)
        .arg(data_dir_arg())
        .subcommand(Command::new("list").about("List users and agents"))
        .subcommand(policy_args(
            Command::new("create")
                .about("Create a user, admin, owner or agent")
                .arg(Arg::new("username").required(true))
                .arg(
                    Arg::new("role")
                        .long("role")
                        .value_parser(["owner", "admin", "user", "agent"])
                        .default_value("user"),
                )
                .arg(
                    Arg::new("password-stdin")
                        .long("password-stdin")
                        .action(ArgAction::SetTrue)
                        .help("Read the password from stdin"),
                )
                .arg(Arg::new("description").long("description")),
        ))
        .subcommand(policy_args(
            Command::new("policy")
                .about("Replace a user's/agent's tool policy")
                .arg(Arg::new("username").required(true)),
        ))
        .subcommand(
            Command::new("set-password")
                .about("Set a user's password")
                .arg(Arg::new("username").required(true))
                .arg(
                    Arg::new("password-stdin")
                        .long("password-stdin")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(Command::new("disable").arg(Arg::new("username").required(true)))
        .subcommand(Command::new("enable").arg(Arg::new("username").required(true)))
        .subcommand(Command::new("delete").arg(Arg::new("username").required(true)))
}

pub fn tokens_cmd() -> Command {
    Command::new("tokens")
        .about("Manage API tokens (for agents, scripts and MCP clients)")
        .subcommand_required(true)
        .arg(data_dir_arg())
        .subcommand(Command::new("list").about("List tokens"))
        .subcommand(policy_args(
            Command::new("create")
                .about("Create a token; scope options narrow the owner's policy")
                .arg(
                    Arg::new("user")
                        .long("user")
                        .required(true)
                        .help("Owner username (user or agent)"),
                )
                .arg(Arg::new("name").long("name").required(true))
                .arg(
                    Arg::new("expires-days")
                        .long("expires-days")
                        .value_parser(clap::value_parser!(i64)),
                ),
        ))
        .subcommand(Command::new("revoke").arg(Arg::new("id").required(true)))
}

pub fn extensions_cmd() -> Command {
    Command::new("extensions")
        .about("Manage declarative tool extensions (TOML manifests)")
        .subcommand_required(true)
        .arg(
            Arg::new("dir")
                .long("dir")
                .help("Extensions directory [default: per-user extensions folder]"),
        )
        .subcommand(Command::new("list").about("List manifests and their trust status"))
        .subcommand(
            Command::new("trust")
                .about("Trust a manifest (pins its current SHA-256)")
                .arg(Arg::new("id").required(true)),
        )
        .subcommand(Command::new("untrust").arg(Arg::new("id").required(true)))
        .subcommand(
            Command::new("install")
                .about(
                    "Install a manifest in one step: validate, copy, trust (pin SHA-256), reload \
                     and run its tests",
                )
                .arg(
                    Arg::new("source")
                        .required(true)
                        .help("Manifest file or http(s) URL"),
                )
                .arg(
                    Arg::new("replace")
                        .long("replace")
                        .action(ArgAction::SetTrue)
                        .help("Replace an installed extension with the same id"),
                )
                .arg(
                    Arg::new("yes")
                        .long("yes")
                        .short('y')
                        .action(ArgAction::SetTrue)
                        .help("Do not ask for confirmation"),
                ),
        )
        .subcommand(
            Command::new("uninstall")
                .about("Remove an extension's manifest and trust entry and reload")
                .arg(Arg::new("id").required(true))
                .arg(
                    Arg::new("purge")
                        .long("purge")
                        .action(ArgAction::SetTrue)
                        .help("Also delete its saved settings"),
                ),
        )
        .subcommand(
            Command::new("test")
                .about("Run an installed extension's tests without reinstalling it")
                .arg(Arg::new("id").required(true)),
        )
        .subcommand(
            Command::new("config")
                .about("Show or change an extension's settings (stored outside its manifest)")
                .arg(Arg::new("id").required(true))
                .arg(Arg::new("values").num_args(0..).help(
                    "key=value pairs; key= clears a setting, key=- reads the value from stdin",
                )),
        )
}

fn open_db(m: &ArgMatches) -> Result<Db, String> {
    let dir = PathBuf::from(m.get_one::<String>("data-dir").unwrap());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Db::open(&dir.join("dogubako.db")).map_err(|e| e.to_string())
}

fn read_password(from_stdin: bool) -> Result<String, String> {
    if from_stdin || !std::io::stdin().is_terminal() {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| e.to_string())?;
        return Ok(s.trim_end_matches(['\r', '\n']).to_string());
    }
    let a = rpassword::prompt_password("Password: ").map_err(|e| e.to_string())?;
    let b = rpassword::prompt_password("Repeat:   ").map_err(|e| e.to_string())?;
    if a != b {
        return Err("passwords do not match".into());
    }
    Ok(a)
}

fn parse_permissions(s: Option<&String>) -> Result<Option<Vec<Permission>>, String> {
    match s {
        None => Ok(None),
        Some(s) if s.trim().is_empty() || s == "none" => Ok(Some(vec![])),
        Some(s) if s == "all" => Ok(Some(Permission::ALL.to_vec())),
        Some(s) => s
            .split(',')
            .map(|p| Permission::parse(p.trim()).ok_or_else(|| format!("unknown permission '{p}'")))
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
    }
}

fn policy_from(m: &ArgMatches, base: Policy) -> Result<Policy, String> {
    let mut p = base;
    if let Some(a) = m.get_many::<String>("allow") {
        p.allow = a.cloned().collect();
    }
    if let Some(d) = m.get_many::<String>("deny") {
        p.deny = d.cloned().collect();
    }
    if let Some(perms) = parse_permissions(m.get_one::<String>("permissions"))? {
        p.permissions = perms;
    }
    if let Some(n) = m.get_one::<usize>("max-jobs") {
        p.max_concurrent_jobs = Some(*n);
    }
    if let Some(n) = m.get_one::<u64>("quota-mb") {
        p.quota_mb = Some(*n);
    }
    if let Some(d) = m.get_one::<String>("discovery") {
        p.mcp_discovery = Some(d.clone());
    }
    p.validate()?;
    Ok(p)
}

pub fn users(m: &ArgMatches, out: &Out) -> i32 {
    let db = match open_db(m) {
        Ok(d) => d,
        Err(e) => return out.error(&e, 1),
    };
    let r: Result<(), String> = (|| match m.subcommand() {
        Some(("list", _)) => {
            let users = db.users();
            if out.json {
                println!("{}", serde_json::to_string_pretty(&users).unwrap());
            } else {
                for u in users {
                    println!(
                        "  {:<24} {:<6} {}{}  allow={} perms={}",
                        u.username,
                        u.role.as_str(),
                        u.id,
                        if u.disabled { " (disabled)" } else { "" },
                        u.policy.allow.join(","),
                        u.policy
                            .permissions
                            .iter()
                            .map(|p| p.id())
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                }
            }
            Ok(())
        }
        Some(("create", s)) => {
            let username = s.get_one::<String>("username").unwrap();
            auth::validate_username(username).map_err(|e| e.message)?;
            let role = Role::parse(s.get_one::<String>("role").unwrap()).unwrap();
            let policy = policy_from(s, Policy::for_role(role))?;
            let hash = if role == Role::Agent {
                None
            } else {
                let pw = read_password(s.get_flag("password-stdin"))?;
                auth::validate_password(&pw).map_err(|e| e.message)?;
                Some(auth::hash_password(&pw)?)
            };
            let id = format!(
                "{}_{}",
                if role == Role::Agent { "a" } else { "u" },
                auth::random_token(8)
            );
            db.create_user(
                &id,
                username,
                hash.as_deref(),
                role,
                &policy,
                s.get_one::<String>("description")
                    .map(|s| s.as_str())
                    .unwrap_or(""),
            )
            .map_err(|e| e.to_string())?;
            db.audit("cli", "user.create", username, "ok", "", role.as_str());
            if out.json {
                println!("{}", json!({"ok": true, "id": id}));
            } else {
                out.info(&format!("created {} '{username}' ({id})", role.as_str()));
                if role == Role::Agent {
                    out.info(&format!(
                        "next: dogubako tokens create --user {username} --name <label>"
                    ));
                }
            }
            Ok(())
        }
        Some(("policy", s)) => {
            let mut u = db
                .user_by_name(s.get_one::<String>("username").unwrap())
                .ok_or("no such user")?;
            u.policy = policy_from(s, u.policy.clone())?;
            db.update_user(&u).map_err(|e| e.to_string())?;
            db.audit(
                "cli",
                "user.update",
                &u.username,
                "ok",
                "",
                &serde_json::to_string(&u.policy).unwrap(),
            );
            out.info(&format!(
                "updated policy of {}: {}",
                u.username,
                serde_json::to_string(&u.policy).unwrap()
            ));
            Ok(())
        }
        Some(("set-password", s)) => {
            let u = db
                .user_by_name(s.get_one::<String>("username").unwrap())
                .ok_or("no such user")?;
            if u.role == Role::Agent {
                return Err("agents use tokens".into());
            }
            let pw = read_password(s.get_flag("password-stdin"))?;
            auth::validate_password(&pw).map_err(|e| e.message)?;
            db.set_password(&u.id, &auth::hash_password(&pw)?)
                .map_err(|e| e.to_string())?;
            out.info("password updated; existing sessions were signed out");
            Ok(())
        }
        Some((action @ ("disable" | "enable"), s)) => {
            let mut u = db
                .user_by_name(s.get_one::<String>("username").unwrap())
                .ok_or("no such user")?;
            u.disabled = action == "disable";
            db.update_user(&u).map_err(|e| e.to_string())?;
            out.info(&format!("{action}d {}", u.username));
            Ok(())
        }
        Some(("delete", s)) => {
            let u = db
                .user_by_name(s.get_one::<String>("username").unwrap())
                .ok_or("no such user")?;
            db.delete_user(&u.id).map_err(|e| e.to_string())?;
            out.info(&format!("deleted {}", u.username));
            Ok(())
        }
        _ => Err("unknown command".into()),
    })();
    match r {
        Ok(()) => 0,
        Err(e) => out.error(&e, 1),
    }
}

pub fn tokens(m: &ArgMatches, out: &Out) -> i32 {
    let db = match open_db(m) {
        Ok(d) => d,
        Err(e) => return out.error(&e, 1),
    };
    let r: Result<(), String> = (|| match m.subcommand() {
        Some(("list", _)) => {
            let t = db.tokens(None);
            if out.json {
                println!("{}", serde_json::to_string_pretty(&t).unwrap());
            } else {
                for t in t {
                    println!(
                        "  {:<12} {:<20} {:<16} {}…{}",
                        t.id,
                        t.username,
                        t.name,
                        t.prefix,
                        if t.revoked { " (revoked)" } else { "" }
                    );
                }
            }
            Ok(())
        }
        Some(("create", s)) => {
            let u = db
                .user_by_name(s.get_one::<String>("user").unwrap())
                .ok_or("no such user")?;
            let has_scope = [
                "allow",
                "deny",
                "permissions",
                "max-jobs",
                "quota-mb",
                "discovery",
            ]
            .iter()
            .any(|k| {
                s.contains_id(k)
                    && s.value_source(k) == Some(clap::parser::ValueSource::CommandLine)
            });
            let scope = if has_scope {
                Some(policy_from(
                    s,
                    Policy {
                        allow: vec!["*".into()],
                        permissions: Permission::ALL.to_vec(),
                        ..Default::default()
                    },
                )?)
            } else {
                None
            };
            let (plain, hash, prefix) = auth::new_api_token();
            let id = format!("t_{}", auth::random_token(8));
            let expires = s
                .get_one::<i64>("expires-days")
                .map(|d| dogubako_server::db::now() + d * 86400);
            db.create_token(
                &id,
                &u.id,
                s.get_one::<String>("name").unwrap(),
                &hash,
                &prefix,
                scope.as_ref(),
                expires,
            )
            .map_err(|e| e.to_string())?;
            db.audit(
                "cli",
                "token.create",
                &format!("{id} for {}", u.username),
                "ok",
                "",
                "",
            );
            if out.json {
                println!("{}", json!({"ok": true, "id": id, "token": plain}));
            } else {
                println!("{plain}");
                out.info(&format!(
                    "token {id} created for {} — store it now, it is not shown again",
                    u.username
                ));
            }
            Ok(())
        }
        Some(("revoke", s)) => {
            let n = db
                .revoke_token(s.get_one::<String>("id").unwrap())
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("no such token".into());
            }
            out.info("revoked");
            Ok(())
        }
        _ => Err("unknown command".into()),
    })();
    match r {
        Ok(()) => 0,
        Err(e) => out.error(&e, 1),
    }
}

fn extensions_dir(m: &ArgMatches) -> PathBuf {
    m.get_one::<String>("dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| dogubako_core::paths::AppDirs::default_local().extensions)
}

/// `extensions install|uninstall|test`.
pub async fn extension_lifecycle(m: &ArgMatches, out: &Out) -> i32 {
    use dogubako_core::extensions as ext;
    let dir = extensions_dir(m);
    let engine = crate::build_engine_with(dogubako_core::tool::Environment::Cli, dir.clone());
    let report = match m.subcommand() {
        Some(("install", s)) => {
            let source = s.get_one::<String>("source").unwrap();
            let opts = ext::InstallOptions {
                replace: s.get_flag("replace"),
            };
            let failed = |id: String, e: String| ext::ExtensionReport {
                id,
                error: Some(e),
                ..Default::default()
            };
            let text = match ext::fetch_manifest(source, &engine.config.net).await {
                Ok(t) => t,
                Err(e) => return print_report(out, &failed(String::new(), e.to_string())),
            };
            let preview = match ext::preview(&engine, &dir, &text, opts) {
                Ok(p) => p,
                Err(e) => {
                    let id = parse_manifest(&text, "")
                        .map(|t| {
                            use dogubako_core::tool::Tool;
                            t.spec().id.clone()
                        })
                        .unwrap_or_default();
                    return print_report(out, &failed(id, e.to_string()));
                }
            };
            if !out.json && !out.quiet {
                eprintln!("{} {} ({})", preview.name, preview.version, preview.id);
                eprintln!("  permissions: {}", none_if_empty(&preview.permissions));
                eprintln!("  requires:    {}", none_if_empty(&preview.requires));
                if !preview.missing.is_empty() {
                    eprintln!("  not installed here: {}", preview.missing.join(", "));
                }
                if preview.replaced {
                    eprintln!("  replaces the installed copy");
                }
            }
            let ask = !s.get_flag("yes")
                && !out.json
                && std::io::stdin().is_terminal()
                && std::io::stderr().is_terminal();
            if ask {
                eprint!("Install and trust this extension? [y/N] ");
                let mut line = String::new();
                let _ = std::io::stdin().read_line(&mut line);
                if !matches!(line.trim(), "y" | "Y" | "yes") {
                    return out.error("cancelled", 1);
                }
            }
            match ext::install(&engine, &dir, &text, opts).await {
                Ok(r) => r,
                Err(e) => failed(preview.id, e.to_string()),
            }
        }
        Some(("test", s)) => {
            let id = s.get_one::<String>("id").unwrap();
            match ext::test(&engine, &dir, id).await {
                Ok(r) => r,
                Err(e) => ext::ExtensionReport {
                    id: id.clone(),
                    error: Some(e.to_string()),
                    ..Default::default()
                },
            }
        }
        Some(("uninstall", s)) => {
            let id = s.get_one::<String>("id").unwrap();
            return match ext::uninstall(&engine, &dir, id, s.get_flag("purge")) {
                Ok(r) => {
                    if out.json {
                        println!("{}", serde_json::to_string_pretty(&r).unwrap());
                    } else {
                        out.info(&format!(
                            "uninstalled {id}{}",
                            if r.purged {
                                " and deleted its settings"
                            } else {
                                " (settings kept; --purge deletes them)"
                            }
                        ));
                    }
                    0
                }
                Err(e) => out.tool_error(&e),
            };
        }
        _ => return 2,
    };
    print_report(out, &report)
}

fn none_if_empty(v: &[String]) -> String {
    if v.is_empty() {
        "none".into()
    } else {
        v.join(", ")
    }
}

fn print_report(out: &Out, r: &dogubako_core::extensions::ExtensionReport) -> i32 {
    use dogubako_core::extensions::TestStatus;
    if out.json {
        println!("{}", serde_json::to_string_pretty(r).unwrap());
    } else {
        if r.loaded {
            println!(
                "{} {} installed: trusted={} loaded={}",
                r.id, r.version, r.trusted, r.loaded
            );
        }
        if !r.unset_settings.is_empty() {
            println!(
                "  set up with: dogubako extensions config {} {}",
                r.id,
                r.unset_settings
                    .iter()
                    .map(|s| format!("{s}=..."))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        if r.loaded && r.tests.is_empty() && r.error.is_none() {
            println!("  no tests in the manifest");
        }
        for t in &r.tests {
            let mark = match t.status {
                TestStatus::Passed => "pass",
                TestStatus::Failed => "FAIL",
                TestStatus::Skipped => "skip",
            };
            match &t.error {
                Some(e) => println!("  {mark} {} — {e}", t.name),
                None => println!("  {mark} {} ({} ms)", t.name, t.duration_ms),
            }
        }
        if let Some(e) = &r.error {
            eprintln!("error: {e}");
        }
    }
    if r.passed {
        0
    } else {
        1
    }
}

pub fn extensions(m: &ArgMatches, out: &Out) -> i32 {
    let dir = extensions_dir(m);
    let trust_path = dir.join("trusted.toml");
    match m.subcommand() {
        Some(("list", _)) => {
            let (_, status) = load_extensions(&dir, false);
            if out.json {
                println!("{}", serde_json::to_string_pretty(&status).unwrap());
            } else {
                println!("Extensions directory: {}", dir.display());
                for s in status {
                    println!(
                        "  {} {:<28} {:<24} {}",
                        if s.loaded { "✓" } else { "·" },
                        s.id.unwrap_or_default(),
                        s.file,
                        s.error.unwrap_or_default()
                    );
                }
            }
            0
        }
        Some(("config", s)) => {
            let id = s.get_one::<String>("id").unwrap();
            let mut changes = serde_json::Map::new();
            let mut stdin_used = false;
            for kv in s.get_many::<String>("values").into_iter().flatten() {
                let Some((k, v)) = kv.split_once('=') else {
                    return out.error(&format!("expected key=value, got '{kv}'"), 2);
                };
                let v = if v == "-" {
                    // Read the value from stdin so secrets stay out of shell history.
                    if stdin_used {
                        return out.error("only one key=- value can be read from stdin", 2);
                    }
                    stdin_used = true;
                    match read_password(true) {
                        Ok(v) if !v.is_empty() => v,
                        Ok(_) => {
                            return out.error(
                                &format!("no value for '{k}' on stdin (use {k}= to clear)"),
                                2,
                            )
                        }
                        Err(e) => return out.error(&e, 1),
                    }
                } else {
                    v.to_string()
                };
                changes.insert(k.trim().to_string(), json!(v));
            }
            let ext = if changes.is_empty() {
                dogubako_core::manifest::extension_settings(&dir)
                    .into_iter()
                    .find(|e| e.id == *id)
                    .ok_or_else(|| {
                        format!("no extension '{id}' with settings in {}", dir.display())
                    })
            } else {
                dogubako_core::manifest::configure_extension(&dir, id, &changes)
                    .map_err(|e| e.to_string())
            };
            let ext = match ext {
                Ok(e) => e,
                Err(e) => return out.error(&e, 1),
            };
            if out.json {
                println!("{}", serde_json::to_string_pretty(&ext).unwrap());
            } else {
                println!("{} ({})", ext.name, ext.id);
                for v in &ext.settings {
                    let shown = match (&v.value, v.secret, v.set) {
                        (_, true, true) => "(secret, set)".to_string(),
                        (Some(serde_json::Value::String(s)), _, _) => s.clone(),
                        (Some(x), _, _) => x.to_string(),
                        _ => "(not set)".to_string(),
                    };
                    let req = if v.spec.required { " *" } else { "" };
                    println!("  {:<20} {shown}{req}", v.spec.name);
                }
            }
            0
        }
        Some((action @ ("trust" | "untrust"), s)) => {
            let id = s.get_one::<String>("id").unwrap();
            let mut store = TrustStore::load(&trust_path);
            if action == "untrust" {
                store.trusted.remove(id);
            } else {
                let found = std::fs::read_dir(&dir).ok().and_then(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.extension().is_some_and(|e| e == "toml")
                                && !p.ends_with("trusted.toml")
                        })
                        .find_map(|p| {
                            let text = std::fs::read_to_string(&p).ok()?;
                            let t = parse_manifest(&text, "").ok()?;
                            use dogubako_core::tool::Tool;
                            (t.spec().id == *id).then(|| (p, sha256_hex(text.as_bytes())))
                        })
                });
                let Some((path, sha)) = found else {
                    return out.error(
                        &format!("no valid manifest with id '{id}' in {}", dir.display()),
                        1,
                    );
                };
                out.info(&format!(
                    "trusting {} ({}) sha256={sha}",
                    id,
                    path.display()
                ));
                store.trusted.insert(id.clone(), sha);
            }
            match store.save(&trust_path) {
                Ok(()) => 0,
                Err(e) => out.error(&e.to_string(), 1),
            }
        }
        _ => 2,
    }
}
