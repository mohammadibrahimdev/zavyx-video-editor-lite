//! zavyx-ve-cli — headless render CLI.
//!
//! Renders `.zvep` projects with the same pipeline as the app's Export
//! dialog (compositor + mixer + bundled ffmpeg.exe), probes media files, and
//! batch-renders folders of projects.

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::Ordering;

use zavyx_ve_media::Prober;
use zavyx_ve_render::export::{self, Format, Quality, State};

#[derive(Parser, Debug)]
#[command(name = "zavyx-ve-cli", version, about = "Zavyx Video Editor — headless render CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Render a .zvep project to a video, audio file or PNG sequence.
    Render {
        /// Path to the .zvep project file.
        #[arg(value_name = "PROJECT")]
        project: PathBuf,
        /// Output file (or folder, for the png preset).
        #[arg(short, long, value_name = "OUTPUT")]
        output: PathBuf,
        /// Render only this range (START-END, in seconds).
        #[arg(short, long, value_name = "START-END")]
        range: Option<String>,
        /// Output height in pixels (width follows the project's aspect). Default: project size.
        #[arg(long, default_value = "0")]
        height: u32,
        /// Frames per second. Default: project frame rate.
        #[arg(long, default_value = "0")]
        fps: f64,
        /// low | medium | high
        #[arg(long, default_value = "medium")]
        quality: String,
        /// Format preset (see `list-presets`). Default: from the output extension.
        #[arg(long, value_name = "PRESET")]
        preset: Option<String>,
    },
    /// Probe a media file and print its info as JSON.
    Probe {
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    /// List all available export presets.
    ListPresets,
    /// Render several .zvep projects (comma-separated list or a folder) into one folder.
    Batch {
        #[arg(value_name = "PROJECTS_OR_DIR")]
        projects: String,
        #[arg(short, long, value_name = "OUTPUT_DIR", default_value = "./batch-out")]
        output: PathBuf,
        /// Format preset (see list-presets).
        #[arg(long, default_value = "mp4")]
        preset: String,
        #[arg(short, long, value_name = "START-END")]
        range: Option<String>,
    },
    /// Print the version of the Zavyx VE build.
    Version,
}

const PRESETS: [(&str, Format); 8] = [
    ("mp4", Format::Mp4), ("mov", Format::Mov), ("mkv", Format::Mkv), ("webm", Format::Webm),
    ("gif", Format::Gif), ("mp3", Format::Mp3), ("wav", Format::Wav), ("png", Format::PngSequence),
];

fn main() -> ExitCode {
    env_logger::init();
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("zavyx-ve-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Render { project, output, range, height, fps, quality, preset } => {
            render(&project, &output, range.as_deref(), height, fps, &quality, preset.as_deref())
        }
        Command::Probe { file } => {
            let info = zavyx_ve_media::cli::CliProber.probe(&file).map_err(|e| anyhow::anyhow!("probe failed: {e}"))?;
            println!("{}", serde_json::to_string_pretty(&info)?);
            Ok(())
        }
        Command::ListPresets => {
            println!("Available export presets:");
            for (id, f) in PRESETS {
                println!("  {:6} {}", id, f.description());
            }
            Ok(())
        }
        Command::Batch { projects, output, preset, range } => batch(&projects, &output, &preset, range.as_deref()),
        Command::Version => {
            println!("zavyx-ve-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

fn parse_range(s: &str) -> anyhow::Result<(f64, f64)> {
    let (a, b) = s.split_once('-').ok_or_else(|| anyhow::anyhow!("invalid range, use START-END in seconds"))?;
    Ok((a.trim().parse()?, b.trim().parse()?))
}

fn preset_for(output: &Path, preset: Option<&str>) -> anyhow::Result<Format> {
    let key = match preset {
        Some(p) => p.to_ascii_lowercase(),
        None => output.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_else(|| "png".into()),
    };
    PRESETS.iter().find(|(id, _)| *id == key).map(|(_, f)| *f)
        .ok_or_else(|| anyhow::anyhow!("unknown preset '{key}'; use `list-presets` to see options"))
}

fn render(project_path: &Path, output: &Path, range: Option<&str>, height: u32, fps: f64, quality: &str, preset: Option<&str>) -> anyhow::Result<()> {
    let format = preset_for(output, preset)?;
    let text = std::fs::read_to_string(project_path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", project_path.display()))?;
    let mut project = zavyx_ve_core::serde::load(&text).map_err(|e| anyhow::anyhow!("failed to parse .zvep: {e}"))?;
    // Media stored relative to the project file still resolves after a move.
    let dir = project_path.parent().map(Path::to_path_buf).unwrap_or_default();
    for (mref, info) in project.media_pool.items.values_mut() {
        if !info.path.is_file() && dir.join(&mref.relative_path).is_file() {
            info.path = dir.join(&mref.relative_path);
        }
    }
    let s = &project.settings;
    let h = if height == 0 { s.height } else { height };
    let w = ((s.width as f64 * h as f64 / s.height.max(1) as f64).round() as u32) & !1;
    let fps = if fps > 0.0 { fps } else { s.time_base.0.to_f64() };
    let quality = match quality { "low" => Quality::Low, "high" => Quality::High, _ => Quality::Medium };
    let settings = export::Settings {
        format, path: output.to_path_buf(), width: w, height: h & !1, fps, quality,
        range: range.map(parse_range).transpose()?,
    };
    let snap = zavyx_ve_render::mixer::snapshot(&project);
    let job = export::start(snap, settings, || {});
    let mut last = u32::MAX;
    loop {
        match job.state() {
            State::Running(_) => {
                let pct = job.progress.load(Ordering::Relaxed) / 10;
                if pct != last && pct % 10 == 0 {
                    println!(r#"{{"progress": {pct}}}"#);
                    last = pct;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            State::Done(path) => {
                println!(r#"{{"done": true, "output": {:?}}}"#, path.display().to_string());
                return Ok(());
            }
            State::Failed(e) => anyhow::bail!(e),
            State::Cancelled => anyhow::bail!("cancelled"),
        }
    }
}

fn batch(projects_arg: &str, output_dir: &Path, preset: &str, range: Option<&str>) -> anyhow::Result<()> {
    let projects: Vec<PathBuf> = if projects_arg.contains(',') {
        projects_arg.split(',').map(|s| PathBuf::from(s.trim())).collect()
    } else {
        let p = PathBuf::from(projects_arg);
        if p.is_dir() {
            std::fs::read_dir(&p)?.filter_map(|e| e.ok()).map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("zvep"))
                .collect()
        } else {
            vec![p]
        }
    };
    if projects.is_empty() {
        anyhow::bail!("no .zvep projects found");
    }
    let format = preset_for(Path::new(""), Some(preset))?;
    std::fs::create_dir_all(output_dir)?;
    let mut failed = 0;
    for (i, proj) in projects.iter().enumerate() {
        let stem = proj.file_stem().and_then(|s| s.to_str()).unwrap_or("project");
        let out = output_dir.join(if format == Format::PngSequence { stem.to_string() } else { format!("{stem}.{}", format.extension()) });
        println!(r#"{{"batch_progress": "{}/{}", "project": {:?}}}"#, i + 1, projects.len(), proj.display().to_string());
        if let Err(e) = render(proj, &out, range, 0, 0.0, "medium", Some(preset)) {
            eprintln!("  failed: {e}");
            failed += 1;
        }
    }
    println!(r#"{{"batch_done": true, "total": {}, "failed": {failed}}}"#, projects.len());
    if failed > 0 { anyhow::bail!("{failed} project(s) failed") } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_comes_from_extension_or_flag() {
        assert_eq!(preset_for(Path::new("a.mp4"), None).unwrap(), Format::Mp4);
        assert_eq!(preset_for(Path::new("out"), None).unwrap(), Format::PngSequence);
        assert_eq!(preset_for(Path::new("a.mp4"), Some("gif")).unwrap(), Format::Gif);
        assert!(preset_for(Path::new("a.xyz"), None).is_err());
    }

    #[test]
    fn ranges_parse() {
        assert_eq!(parse_range("1.5-3").unwrap(), (1.5, 3.0));
        assert!(parse_range("abc").is_err());
    }
}
