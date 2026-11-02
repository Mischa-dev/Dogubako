//! `dogubako` — command-line interface.
//!
//! Tool commands (`dogubako image convert …`) are generated at runtime from
//! the same tool specifications that drive the UI, HTTP API and MCP.

mod admin;
mod local;
mod mcp_stdio;
mod render;
mod toolcmd;

use clap::{Arg, ArgAction, ArgMatches, Command};
use dogubako_core::engine::{Engine, EngineConfig};
use dogubako_core::paths::AppDirs;
use dogubako_core::spec::Category;
use dogubako_core::tool::Environment;
use std::process::ExitCode;

pub fn build_engine(env: Environment) -> Engine {
    let dirs = AppDirs::default_local();
    let (registry, _) = dogubako_tools::build_registry(Some(&dirs.extensions));
    let mut cfg = EngineConfig::local(env);
    cfg.models_dir = std::env::var_os("DOGUBAKO_MODELS_DIR").map(Into::into).unwrap_or(dirs.models.clone());
    Engine::new(registry, cfg, vec![dirs.bin])
}

fn global_args(cmd: Command) -> Command {
    cmd.arg(Arg::new("json").long("json").global(true).action(ArgAction::SetTrue).help("Machine-readable JSON output"))
        .arg(Arg::new("quiet").long("quiet").short('q').global(true).action(ArgAction::SetTrue).help("Suppress progress and informational messages"))
        .arg(Arg::new("verbose").long("verbose").short('v').global(true).action(ArgAction::Count).help("Increase log verbosity"))
}

fn cli(engine: &Engine) -> Command {
    let mut cmd = Command::new("dogubako")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Dogubako — a local-first, open-source utility toolbox")
        .long_about(
            "Dogubako — a local-first, open-source utility toolbox.\n\n\
             Run any tool as `dogubako <category> <tool> [inputs] [--options]`, e.g.\n  \
             dogubako image convert photo.png --format webp\n  \
             dogubako pdf merge a.pdf b.pdf -o merged.pdf\n  \
             echo '{\"a\":1}' | dogubako dev json\n\n\
             Discover tools with `dogubako tools list` / `dogubako tools search <words>`.",
        )
        .subcommand_required(true)
        .arg_required_else_help(true);
    cmd = global_args(cmd);

    cmd = cmd.subcommand(
        Command::new("tools")
            .about("List, search and describe tools")
            .subcommand_required(true)
            .subcommand(Command::new("list").about("List tools").arg(Arg::new("category").long("category").short('c').help("Filter by category")).arg(Arg::new("available").long("available").action(ArgAction::SetTrue).help("Only tools usable on this machine")))
            .subcommand(Command::new("search").about("Search tools by keyword").arg(Arg::new("query").num_args(1..).required(true)))
            .subcommand(Command::new("info").about("Show a tool's parameters, dependencies and schema").arg(Arg::new("tool").required(true)))
            .subcommand(Command::new("matrix").about("Print a Markdown capability matrix of all tools (platforms, dependencies, permissions)")),
    );
    cmd = cmd.subcommand(
        Command::new("run")
            .about("Run any tool by id with key=value parameters")
            .arg(Arg::new("tool").required(true).help("Tool id, e.g. image.resize"))
            .arg(Arg::new("set").long("set").short('s').action(ArgAction::Append).value_name("KEY=VALUE").help("Parameter value (repeatable). Lists: comma-separated; JSON allowed."))
            .arg(Arg::new("params").long("params").value_name("JSON").help("All parameters as a JSON object"))
            .arg(toolcmd::output_arg())
            .arg(toolcmd::force_arg()),
    );
    // Generated category / tool commands.
    for cat in Category::ALL {
        let specs: Vec<_> = engine.registry.by_category(cat).collect();
        if specs.is_empty() {
            continue;
        }
        let mut c = Command::new(cat.id()).about(format!("{} — {}", cat.label(), cat.description())).subcommand_required(true);
        for spec in specs {
            c = c.subcommand(toolcmd::command_for(spec, &engine.availability(spec)));
        }
        cmd = cmd.subcommand(c);
    }
    cmd = cmd.subcommand(
        Command::new("server")
            .about("Run the self-hosted web service (web UI + HTTP API + MCP)")
            .subcommand_required(true)
            .subcommand(
                Command::new("start")
                    .about("Start the server")
                    .arg(Arg::new("bind").long("bind").short('b').env("DOGUBAKO_BIND").help("Listen address [default: 127.0.0.1:8420]"))
                    .arg(Arg::new("data-dir").long("data-dir").short('d').env("DOGUBAKO_DATA_DIR").help("Data directory [default: ./data]"))
                    .arg(Arg::new("config").long("config").env("DOGUBAKO_CONFIG").help("TOML config file"))
                    .arg(Arg::new("no-auth").long("no-auth").action(ArgAction::SetTrue).help("Disable authentication (loopback only)"))
                    .arg(Arg::new("public-url").long("public-url").env("DOGUBAKO_PUBLIC_URL").help("Externally visible base URL")),
            )
            .subcommand(Command::new("config").about("Print the effective configuration as TOML").arg(Arg::new("config").long("config")))
            .subcommand(Command::new("health").about("Check that a server is responding (for container health checks)").arg(Arg::new("addr").long("addr").env("DOGUBAKO_BIND").default_value("127.0.0.1:8420"))),
    );
    cmd = cmd.subcommand(
        Command::new("mcp")
            .about("Model Context Protocol server")
            .subcommand_required(true)
            .subcommand(
                Command::new("serve")
                    .about("Serve MCP over stdio for a local agent (local paths in, files out)")
                    .arg(Arg::new("allow").long("allow").action(ArgAction::Append).help("Allowed tool patterns (default: *), e.g. 'image.*'"))
                    .arg(Arg::new("deny").long("deny").action(ArgAction::Append).help("Denied tool patterns"))
                    .arg(Arg::new("permissions").long("permissions").help("Comma-separated capabilities to grant (default: all): network,host_fs_read,host_fs_write,system_info,process_info"))
                    .arg(Arg::new("discovery").long("discovery").value_parser(["full", "compact"]).default_value("full"))
                    .arg(Arg::new("output-dir").long("output-dir").help("Where output files are written"))
                    .arg(Arg::new("fs-root").long("fs-root").action(ArgAction::Append).help("Restrict file inputs and path parameters to these folders")),
            ),
    );
    cmd = cmd.subcommand(Command::new("deps").about("Show external dependency status and install hints"));
    cmd = cmd.subcommand(
        Command::new("models")
            .about("Manage optional local models (speech recognition, face detection, voices)")
            .subcommand_required(true)
            .subcommand(Command::new("list").about("List models"))
            .subcommand(Command::new("download").about("Download and verify a model").arg(Arg::new("id").required(true)))
            .subcommand(Command::new("remove").about("Remove a downloaded model").arg(Arg::new("id").required(true))),
    );
    cmd = cmd.subcommand(admin::users_cmd()).subcommand(admin::tokens_cmd()).subcommand(admin::extensions_cmd());
    cmd = cmd.subcommand(Command::new("api").about("API utilities").subcommand_required(true).subcommand(Command::new("openapi").about("Print the OpenAPI document")));
    cmd
}

fn init_tracing(verbose: u8, quiet: bool) {
    let default = match (quiet, verbose) {
        (true, _) => "error",
        (_, 0) => "warn,dogubako_server=info,tower_http=warn",
        (_, 1) => "info",
        _ => "debug",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_env("DOGUBAKO_LOG").unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).try_init();
}

fn main() -> ExitCode {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    rt.block_on(async_main())
}

async fn async_main() -> ExitCode {
    let engine = build_engine(Environment::Cli);
    let matches = cli(&engine).get_matches();
    let json = matches.get_flag("json");
    let quiet = matches.get_flag("quiet");
    init_tracing(matches.get_count("verbose"), quiet || json);
    let ctx = render::Out { json, quiet };
    let code = dispatch(engine, &matches, &ctx).await;
    ExitCode::from(code.clamp(0, 255) as u8)
}

async fn dispatch(engine: Engine, m: &ArgMatches, out: &render::Out) -> i32 {
    match m.subcommand() {
        Some(("tools", sub)) => toolcmd::tools_cmd(&engine, sub, out),
        Some(("run", sub)) => toolcmd::run_generic(engine, sub, out).await,
        Some(("server", sub)) => match sub.subcommand() {
            Some(("start", s)) => server_start(s).await,
            Some(("health", s)) => health(s.get_one::<String>("addr").unwrap()),
            Some(("config", s)) => {
                match dogubako_server::ServerConfig::load(s.get_one::<String>("config").map(Into::into)) {
                    Ok(c) => {
                        println!("{}", toml_string(&c));
                        0
                    }
                    Err(e) => out.error(&e.to_string(), 2),
                }
            }
            _ => 2,
        },
        Some(("mcp", sub)) => match sub.subcommand() {
            Some(("serve", s)) => mcp_stdio::serve(engine, s).await,
            _ => 2,
        },
        Some(("deps", _)) => render::deps(&engine, out),
        Some(("models", sub)) => local::models(&engine, sub, out).await,
        Some(("users", sub)) => admin::users(sub, out),
        Some(("tokens", sub)) => admin::tokens(sub, out),
        Some(("extensions", sub)) => admin::extensions(sub, out),
        Some(("api", sub)) => match sub.subcommand() {
            Some(("openapi", _)) => {
                println!("{}", serde_json::to_string_pretty(&dogubako_server::openapi::spec(&engine)).unwrap());
                0
            }
            _ => 2,
        },
        Some((cat, sub)) if Category::parse(cat).is_some() => match sub.subcommand() {
            Some((tool, tm)) => {
                let id = format!("{cat}.{}", tool.replace('-', "_"));
                toolcmd::run_generated(engine, &id, tm, out).await
            }
            None => 2,
        },
        _ => 2,
    }
}

fn toml_string(c: &dogubako_server::ServerConfig) -> String {
    let mut v = serde_json::to_value(c).unwrap();
    if let Some(o) = v.as_object_mut() {
        o.retain(|_, v| !v.is_null());
    }
    let mut s = String::new();
    for (k, v) in v.as_object().unwrap() {
        s.push_str(&format!("{k} = {}\n", toml_value(v)));
    }
    s
}

fn toml_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => format!("{s:?}"),
        serde_json::Value::Array(a) => format!("[{}]", a.iter().map(toml_value).collect::<Vec<_>>().join(", ")),
        other => other.to_string(),
    }
}

fn health(addr: &str) -> i32 {
    use std::io::{Read, Write};
    let addr = addr.replace("0.0.0.0", "127.0.0.1").replace("[::]", "[::1]");
    let Ok(sa) = addr.parse::<std::net::SocketAddr>() else { return 2 };
    let Ok(mut s) = std::net::TcpStream::connect_timeout(&sa, std::time::Duration::from_secs(3)) else { return 1 };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(3)));
    if s.write_all(format!("GET /healthz HTTP/1.0\r\nHost: {addr}\r\n\r\n").as_bytes()).is_err() {
        return 1;
    }
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    if buf.starts_with("HTTP/1.1 200") || buf.starts_with("HTTP/1.0 200") {
        0
    } else {
        1
    }
}

async fn server_start(m: &ArgMatches) -> i32 {
    let mut cfg = match dogubako_server::ServerConfig::load(m.get_one::<String>("config").map(Into::into)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    if let Some(b) = m.get_one::<String>("bind") {
        match b.parse() {
            Ok(a) => cfg.bind = a,
            Err(e) => {
                eprintln!("error: invalid --bind '{b}': {e}");
                return 2;
            }
        }
    }
    if let Some(d) = m.get_one::<String>("data-dir") {
        cfg.data_dir = d.into();
    }
    if let Some(u) = m.get_one::<String>("public-url") {
        cfg.public_url = Some(u.trim_end_matches('/').to_string());
    }
    if m.get_flag("no-auth") {
        cfg.auth = dogubako_server::config::AuthMode::None;
    }
    let _ = tracing_subscriber::fmt().try_init();
    match dogubako_server::serve(cfg).await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}
