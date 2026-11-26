//! Developer CLI (`cargo rugkit dev`) for rugkit apps.
//!
//! Runs your app with `subsecond` hot-patching (via `dx serve --hotpatch` when `dioxus-cli`
//! is available on `PATH`, enabling zero-restart function patching that keeps all rugkit
//! signal state alive), or falls back to a built-in `notify` file watcher that rebuilds and
//! restarts automatically on `.rs` / `Cargo.toml` saves.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand};
use notify::{Event, EventKind, RecursiveMode, Watcher};

/// Top-level CLI invoked as `cargo rugkit <command>` or `cargo-rugkit <command>`.
#[derive(Parser, Debug, PartialEq, Eq)]
#[command(
    name = "cargo-rugkit",
    bin_name = "cargo rugkit",
    about = "Rugkit developer CLI with subsecond hot-reload"
)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Commands,
}

/// Supported `cargo rugkit` subcommands.
#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Commands {
    /// Run a rugkit app with live hot-reload (watches source files and patches views while
    /// keeping signal state).
    Dev(DevArgs),
    /// Print detected hot-reload backend (`subsecond` / `dx` vs file-watcher) and environment.
    Info,
}

/// Arguments for `cargo rugkit dev`.
#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct DevArgs {
    /// Run an example target (equivalent to `cargo run --example <name>`).
    #[arg(long)]
    pub example: Option<String>,
    /// Run a specific binary target (equivalent to `cargo run --bin <name>`).
    #[arg(long)]
    pub bin: Option<String>,
    /// Package to run in a workspace (`-p <pkg>`).
    #[arg(short, long)]
    pub package: Option<String>,
    /// Additional Cargo features to enable (comma-separated).
    #[arg(long)]
    pub features: Option<String>,
    /// Build in release mode.
    #[arg(long)]
    pub release: bool,
    /// Disable `subsecond` (`dx serve --hotpatch`) delegation and use the built-in watcher.
    #[arg(long)]
    pub no_hotpatch: bool,
    /// Paths to watch for changes (defaults to `src`, `crates`, `examples`, `Cargo.toml`).
    #[arg(long = "watch")]
    pub watch_paths: Vec<PathBuf>,
}

/// Parses CLI arguments, stripping the optional `"rugkit"` subcommand token that Cargo injects
/// when invoked as `cargo rugkit ...`.
pub fn parse_args<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut collected: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
    if collected.get(1).and_then(|s| s.to_str()) == Some("rugkit") {
        collected.remove(1);
    }
    Cli::try_parse_from(collected)
}

/// Builds the `cargo run` argument list for [`DevArgs`].
pub fn cargo_run_args(args: &DevArgs, enable_hot_reload_feature: bool) -> Vec<String> {
    let mut out = vec!["run".to_string()];
    if args.release {
        out.push("--release".to_string());
    }
    if let Some(pkg) = &args.package {
        out.push("-p".to_string());
        out.push(pkg.clone());
    }
    if let Some(ex) = &args.example {
        out.push("--example".to_string());
        out.push(ex.clone());
    }
    if let Some(bin) = &args.bin {
        out.push("--bin".to_string());
        out.push(bin.clone());
    }
    let mut feats: Vec<String> = args
        .features
        .as_deref()
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if enable_hot_reload_feature && !feats.iter().any(|f| f == "hot-reload") {
        feats.push("hot-reload".to_string());
    }
    if !feats.is_empty() {
        out.push("--features".to_string());
        out.push(feats.join(","));
    }
    out
}

/// Returns `true` if a filesystem change at `path` should trigger a hot-reload / rebuild.
pub fn should_reload_path(path: &Path) -> bool {
    if path.components().any(|c| {
        let s = c.as_os_str();
        s == "target" || s == ".git" || s == "book"
    }) {
        return false;
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs" | "toml") => true,
        _ => path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == "Cargo.toml"),
    }
}

/// Returns `true` if `dx` (Dioxus CLI with Subsecond's ThinLinker hot-patch server) is on `PATH`.
pub fn has_subsecond_dx() -> bool {
    Command::new("dx")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Entry point called by `cargo-rugkit`.
pub fn run_cli(cli: Cli) -> ExitCode {
    match cli.command {
        Commands::Info => {
            println!("rugkit-cli {}", env!("CARGO_PKG_VERSION"));
            if has_subsecond_dx() {
                println!("hot-reload backend: subsecond (dx serve --hotpatch detected)");
            } else {
                println!(
                    "hot-reload backend: built-in notify watcher (install `dioxus-cli` for subsecond binary hot-patching)"
                );
            }
            ExitCode::SUCCESS
        }
        Commands::Dev(args) => run_dev(&args),
    }
}

fn run_dev(args: &DevArgs) -> ExitCode {
    if !args.no_hotpatch && has_subsecond_dx() {
        eprintln!("rugkit dev: starting subsecond hot-patch session via `dx serve --hotpatch`…");
        let mut cmd = Command::new("dx");
        cmd.args(["serve", "--hotpatch"]);
        if args.release {
            cmd.arg("--release");
        }
        if let Some(pkg) = &args.package {
            cmd.args(["-p", pkg]);
        }
        if let Some(ex) = &args.example {
            cmd.args(["--example", ex]);
        }
        if let Some(bin) = &args.bin {
            cmd.args(["--bin", bin]);
        }
        let mut feats = args.features.clone().unwrap_or_default();
        if !feats.split(',').any(|f| f.trim() == "hot-reload") {
            if !feats.is_empty() {
                feats.push(',');
            }
            feats.push_str("hot-reload");
        }
        cmd.args(["--features", &feats]);
        return match cmd.status() {
            Ok(status) if status.success() => ExitCode::SUCCESS,
            Ok(_) | Err(_) => ExitCode::FAILURE,
        };
    }

    eprintln!(
        "rugkit dev: watching workspace files (tip: install `dioxus-cli` for zero-restart subsecond hot-patching)"
    );
    run_watcher_loop(args)
}

fn run_watcher_loop(args: &DevArgs) -> ExitCode {
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let Ok(mut watcher) = notify::recommended_watcher(tx) else {
        eprintln!("rugkit dev: failed to initialise file watcher");
        return ExitCode::FAILURE;
    };

    let watch_roots: Vec<PathBuf> = if args.watch_paths.is_empty() {
        ["src", "crates", "examples", "Cargo.toml"]
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect()
    } else {
        args.watch_paths.clone()
    };
    for root in &watch_roots {
        let _ = watcher.watch(root, RecursiveMode::Recursive);
    }

    let cargo_args = cargo_run_args(args, false);
    let mut child = spawn_cargo(&cargo_args);
    let mut last_restart = Instant::now();

    for event in rx {
        let Ok(ev) = event else { continue };
        let relevant = matches!(
            ev.kind,
            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
        ) && ev.paths.iter().any(|p| should_reload_path(p));
        if !relevant || last_restart.elapsed() < Duration::from_millis(250) {
            continue;
        }
        last_restart = Instant::now();
        eprintln!("rugkit dev: change detected, rebuilding…");
        if let Some(mut proc) = child.take() {
            let _ = proc.kill();
            let _ = proc.wait();
        }
        child = spawn_cargo(&cargo_args);
    }
    ExitCode::SUCCESS
}

fn spawn_cargo(args: &[String]) -> Option<Child> {
    Command::new("cargo")
        .args(args)
        .spawn()
        .map_err(|e| eprintln!("rugkit dev: failed to launch cargo: {e}"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cargo_subcommand_invocation() {
        let cli = parse_args(["cargo-rugkit", "rugkit", "dev", "--example", "counter"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Dev(DevArgs {
                example: Some("counter".into()),
                bin: None,
                package: None,
                features: None,
                release: false,
                no_hotpatch: false,
                watch_paths: Vec::new(),
            })
        );
    }

    #[test]
    fn builds_cargo_run_args_with_hot_reload_feature() {
        let args = DevArgs {
            example: Some("counter".into()),
            bin: None,
            package: Some("rugkit".into()),
            features: Some("icons".into()),
            release: false,
            no_hotpatch: false,
            watch_paths: Vec::new(),
        };
        let built = cargo_run_args(&args, true);
        assert_eq!(
            built,
            vec![
                "run",
                "-p",
                "rugkit",
                "--example",
                "counter",
                "--features",
                "icons,hot-reload"
            ]
        );
    }

    #[test]
    fn filters_watch_paths_correctly() {
        assert!(should_reload_path(Path::new("crates/rugkit/src/lib.rs")));
        assert!(should_reload_path(Path::new("Cargo.toml")));
        assert!(!should_reload_path(Path::new("target/debug/counter.rs")));
        assert!(!should_reload_path(Path::new(".git/index")));
        assert!(!should_reload_path(Path::new("docs/screenshots/m4.png")));
    }
}
