mod convert;
mod gui;
mod integrate;
mod preset;
mod tools;

use clap::{Parser, Subcommand};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;

/// Convert and compress files from your file manager's context menu.
/// Without a command, opens the settings window.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Convert files with a preset.
    Convert {
        #[arg(long, alias = "conversion-preset")]
        preset: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// List presets (only those accepting every given file, if any).
    List { files: Vec<PathBuf> },
    /// Ask which preset to use (macOS Finder Quick Action entry point), then convert.
    Pick {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Add the context menu entries to the file manager.
    Install {
        /// Default presets for all users (Linux packages, as root).
        #[arg(long, hide = true)]
        system: bool,
    },
    /// Remove the context menu entries.
    Uninstall {
        #[arg(long, hide = true)]
        system: bool,
    },
    /// Print the presets file path (created with the defaults if missing).
    Config,
    /// Open the settings window (presets, context menu entries, conversion options).
    Settings {
        /// Ignored: file managers pass the selected files to the "Configure presets…" entry.
        #[arg(hide = true)]
        files: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().cmd.unwrap_or(Cmd::Settings { files: vec![] })) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            notify("File Converter", &e);
            ExitCode::FAILURE
        }
    }
}

fn run(cmd: Cmd) -> Result<ExitCode, String> {
    let print = |msg: String| {
        println!("{msg}");
        ExitCode::SUCCESS
    };
    // Before loading presets: package scripts run as root and must not create root's presets file.
    match cmd {
        Cmd::Install { system: true } => return integrate::install_system().map(print),
        Cmd::Uninstall { system: true } => return integrate::uninstall_system().map(print),
        _ => {}
    }
    let cfg = preset::load()?;
    match cmd {
        Cmd::Convert { preset, files } => {
            let p = cfg
                .presets
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&preset))
                .ok_or_else(|| format!("no preset named '{preset}' (see `file-converter list`)"))?;
            Ok(convert_all(p, files, cfg.max_simultaneous))
        }
        Cmd::List { files } => {
            for p in cfg.presets.iter().filter(|p| files.iter().all(|f| p.accepts(f))) {
                println!("{}", p.name);
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Pick { files } => {
            let mut names: Vec<&str> =
                cfg.presets.iter().filter(|p| files.iter().all(|f| p.accepts(f))).map(|p| p.name.as_str()).collect();
            names.push(integrate::SETTINGS_LABEL);
            let Some(name) = integrate::choose(&names, files.len())? else {
                return Ok(ExitCode::SUCCESS);
            };
            if name == integrate::SETTINGS_LABEL {
                return gui::run().map(|_| ExitCode::SUCCESS);
            }
            let p = cfg.presets.iter().find(|p| p.name == name).unwrap();
            Ok(convert_all(p, files, cfg.max_simultaneous))
        }
        Cmd::Install { .. } => integrate::install(&cfg.presets).map(|mut msg| {
            let missing: Vec<String> = tools::missing().iter().map(|(t, why)| format!("{t} ({why})")).collect();
            if !missing.is_empty() {
                msg +=
                    &format!("\nNot installed: {}\nInstall them with: {}", missing.join(", "), tools::install_hint());
            }
            print(msg)
        }),
        Cmd::Uninstall { .. } => integrate::uninstall().map(print),
        Cmd::Config => {
            println!("{}", preset::config_path().display());
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Settings { .. } => {
            hide_console();
            gui::run().map(|_| ExitCode::SUCCESS)
        }
    }
}

/// The settings window needs no console; on Windows a console-subsystem exe always gets one.
fn hide_console() {
    #[cfg(windows)]
    {
        extern "system" {
            fn FreeConsole() -> i32;
        }
        // SAFETY: plain kernel32 call without arguments.
        unsafe { FreeConsole() };
    }
}

fn convert_all(p: &preset::Preset, files: Vec<PathBuf>, max: usize) -> ExitCode {
    notify("File Converter", &format!("Converting {} file(s): {}", files.len(), p.name));
    let multi = MultiProgress::new();
    let style = ProgressStyle::with_template("{prefix:30!} [{bar:30}] {pos:>3}% {msg}").unwrap().progress_chars("=> ");
    let queue: Mutex<VecDeque<(PathBuf, ProgressBar)>> = Mutex::new(
        files
            .iter()
            .map(|f| {
                let bar = multi.add(ProgressBar::new(100).with_style(style.clone()));
                bar.set_prefix(f.file_name().unwrap_or_default().to_string_lossy().into_owned());
                bar.set_message("waiting");
                (f.clone(), bar)
            })
            .collect(),
    );
    let results = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..max.clamp(1, files.len()) {
            s.spawn(|| loop {
                let Some((file, bar)) = queue.lock().unwrap().pop_front() else { break };
                let res = convert::convert(&file, p, &bar);
                match &res {
                    Ok(_) => bar.finish_with_message("done"),
                    Err(_) => bar.abandon_with_message("failed"),
                }
                results.lock().unwrap().push((file, res));
            });
        }
    });

    let results = results.into_inner().unwrap();
    let failed: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();
    for (file, res) in &results {
        match res {
            Ok(outs) => {
                for o in outs {
                    eprintln!("ok   {} -> {}", file.display(), o.display());
                }
            }
            Err(e) => eprintln!("FAIL {}: {e}", file.display()),
        }
    }
    if failed.is_empty() {
        notify("File Converter", &format!("{} file(s) converted: {}", results.len(), p.name));
        ExitCode::SUCCESS
    } else {
        let (file, err) = failed[0];
        let first = format!("{}: {}", file.display(), err.as_ref().unwrap_err());
        notify(&format!("File Converter: {} of {} failed", failed.len(), results.len()), &first);
        ExitCode::FAILURE
    }
}

/// Desktop notification; failures are ignored (no notification daemon, headless, ...).
fn notify(summary: &str, body: &str) {
    let body: String = body.chars().take(400).collect();
    #[cfg(target_os = "macos")]
    {
        let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!("display notification \"{}\" with title \"{}\"", q(&body), q(summary));
        let _ = std::process::Command::new("osascript").args(["-e", &script]).output();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = notify_rust::Notification::new().summary(summary).body(&body).appname("File Converter").show();
    }
}

#[cfg(test)]
#[test]
fn settings_accepts_file_manager_args() {
    // Dolphin appends the selected files to "Configure presets…"; rejecting them meant no window.
    let cli = Cli::try_parse_from(["file-converter", "settings", "/a b.png", "/c.png"]).unwrap();
    assert!(matches!(cli.cmd, Some(Cmd::Settings { .. })));
    assert!(Cli::try_parse_from(["file-converter", "settings"]).is_ok());
}
