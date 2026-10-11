//! AdNox command-line tool.
//!
//! Usage:
//!   adnox update [--dir DIR]                  Download the default filter lists into DIR.
//!   adnox run    [--dir DIR] [--bind ADDR]    Load lists from DIR and run the DNS blocker.
//!                [--upstream ADDR] [--list F]
//!   adnox test   <domain> [--dir DIR]         Report whether a domain would be blocked.
//!   adnox lists                               Print the default filter-list sources.
//!
//! The bundled seed list is always loaded, so `adnox run` and `adnox test` work even before
//! `adnox update` has fetched the full lists.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use adnox_core::Blocklist;
use adnox_dns::{run_udp, ServerConfig, Stats};

/// Bundled seed list — always available, even offline.
const SEED: &str = include_str!("seed.txt");

/// Default upstream filter-list sources downloaded by `adnox update`.
const DEFAULT_SOURCES: &[(&str, &str)] = &[
    (
        "stevenblack-hosts.txt",
        "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts",
    ),
    (
        "peter-lowe.txt",
        "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=hosts&showintro=0&mimetype=plaintext",
    ),
    (
        "adguard-dns.txt",
        "https://raw.githubusercontent.com/AdguardTeam/FiltersRegistry/master/filters/filter_15_DnsFilter/filter.txt",
    ),
    ("easylist.txt", "https://easylist.to/easylist/easylist.txt"),
    (
        "easyprivacy.txt",
        "https://easylist.to/easylist/easyprivacy.txt",
    ),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");

    let result = match cmd {
        "update" => cmd_update(&args[1..]),
        "run" => cmd_run(&args[1..]),
        "test" => cmd_test(&args[1..]),
        "lists" => {
            cmd_lists();
            Ok(())
        }
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown command: {other}\n\n{HELP}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

const HELP: &str = "\
AdNox — ad & tracker blocker (DNS engine)

USAGE:
  adnox update [--dir DIR]
  adnox run    [--dir DIR] [--bind ADDR] [--upstream ADDR] [--list FILE]...
  adnox test   <domain> [--dir DIR]
  adnox lists

OPTIONS:
  --dir DIR         Directory holding downloaded filter lists   [default: ./adnox-lists]
  --bind ADDR       Address the DNS server listens on           [default: 127.0.0.1:53]
  --upstream ADDR   Upstream resolver for allowed queries       [default: 1.1.1.1:53]
  --list FILE       Extra filter-list file to load (repeatable)

NOTE: binding to port 53 usually needs elevated privileges. For a quick test use
      --bind 127.0.0.1:5353 and query it with: dig @127.0.0.1 -p 5353 doubleclick.net";

fn print_help() {
    println!("{HELP}");
}

fn cmd_lists() {
    println!("Default filter-list sources (downloaded by `adnox update`):\n");
    for (file, url) in DEFAULT_SOURCES {
        println!("  {file:<22} {url}");
    }
}

// Option parsing helpers --------------------------------------------------------------------

fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn opt_all<'a>(args: &'a [String], name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            if let Some(v) = args.get(i + 1) {
                out.push(v.as_str());
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn list_dir(args: &[String]) -> PathBuf {
    PathBuf::from(opt(args, "--dir").unwrap_or("./adnox-lists"))
}

// Commands ----------------------------------------------------------------------------------

fn cmd_update(args: &[String]) -> Result<(), String> {
    let dir = list_dir(args);
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let agent = build_http_agent();
    let mut ok = 0usize;
    for (file, url) in DEFAULT_SOURCES {
        print!("Downloading {url} ... ");
        use std::io::Write;
        std::io::stdout().flush().ok();
        match download(&agent, url) {
            Ok(body) => {
                let path = dir.join(file);
                fs::write(&path, &body)
                    .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
                println!("{} bytes", body.len());
                ok += 1;
            }
            Err(e) => println!("FAILED ({e})"),
        }
    }
    println!(
        "\nUpdated {ok}/{} lists into {}",
        DEFAULT_SOURCES.len(),
        dir.display()
    );
    if ok == 0 {
        return Err("no lists could be downloaded (check your network connection)".into());
    }
    Ok(())
}

fn cmd_test(args: &[String]) -> Result<(), String> {
    let dir_val = opt(args, "--dir").unwrap_or("");
    let domain = args
        .iter()
        .find(|a| !a.starts_with("--") && a.as_str() != dir_val)
        .ok_or("usage: adnox test <domain> [--dir DIR]")?;
    let bl = load_blocklist(&list_dir(args), &[])?;
    println!(
        "{domain}: {}",
        if bl.is_blocked_domain(domain) {
            "BLOCKED"
        } else {
            "allowed"
        }
    );
    Ok(())
}

fn cmd_run(args: &[String]) -> Result<(), String> {
    let dir = list_dir(args);
    let extra = opt_all(args, "--list");
    let bl = load_blocklist(&dir, &extra)?;

    let bind = opt(args, "--bind").unwrap_or("127.0.0.1:53");
    let upstream = opt(args, "--upstream").unwrap_or("1.1.1.1:53");
    let cfg = Arc::new(ServerConfig {
        bind: bind
            .parse()
            .map_err(|_| format!("invalid --bind address: {bind}"))?,
        upstream: upstream
            .parse()
            .map_err(|_| format!("invalid --upstream address: {upstream}"))?,
        sinkhole_ttl: 60,
    });
    let stats = Arc::new(Stats::default());

    println!(
        "AdNox DNS blocker: {} domains loaded, {} rules.",
        bl.domain_count(),
        bl.rule_count()
    );
    println!("Listening on {} -> upstream {}", cfg.bind, cfg.upstream);
    println!("Press Ctrl-C to stop.");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start async runtime: {e}"))?;

    rt.block_on(async move {
        // periodic stats printer
        let stats_printer = stats.clone();
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(std::time::Duration::from_secs(30));
            iv.tick().await;
            loop {
                iv.tick().await;
                let s = stats_printer.snapshot();
                println!(
                    "[stats] queries={} blocked={} forwarded={} errors={}",
                    s.queries, s.blocked, s.forwarded, s.errors
                );
            }
        });

        tokio::select! {
            r = run_udp(&bl, cfg, stats.clone()) => {
                if let Err(e) = r {
                    eprintln!("server error: {e}");
                }
            }
            _ = tokio::signal::ctrl_c() => {
                let s = stats.snapshot();
                println!(
                    "\nShutting down. Total: queries={} blocked={} forwarded={} errors={}",
                    s.queries, s.blocked, s.forwarded, s.errors
                );
            }
        }
    });

    Ok(())
}

// Loading ------------------------------------------------------------------------------------

fn load_blocklist(dir: &Path, extra: &[&str]) -> Result<Blocklist, String> {
    let mut lists: Vec<String> = vec![SEED.to_string()];

    if dir.is_dir() {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        entries.sort();
        for path in entries {
            if let Ok(body) = fs::read_to_string(&path) {
                lists.push(body);
            }
        }
    }
    for f in extra {
        let body = fs::read_to_string(f).map_err(|e| format!("cannot read {f}: {e}"))?;
        lists.push(body);
    }

    Ok(Blocklist::from_lists(&lists))
}

// HTTP ---------------------------------------------------------------------------------------

fn build_http_agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("AdNox/0.1 (+https://github.com/Dhanancheyan43/AdNox)");
    // Honour a proxy from the environment if one is set (used in some build/CI sandboxes).
    for var in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(p) = std::env::var(var) {
            if let Ok(proxy) = ureq::Proxy::new(&p) {
                builder = builder.proxy(proxy);
                break;
            }
        }
    }
    builder.build()
}

fn download(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    use std::io::Read;
    resp.into_reader()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}
