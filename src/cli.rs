//! Argument dispatch and the one-shot command line run.

use crate::build::{BuildOptions, BuildReport, Mode, build_playlist, resolve};
use clap::{CommandFactory, Parser, error::ErrorKind};
use std::path::PathBuf;

const EPILOG: &str = "\
Examples:
  playlistgenrs /media/tv
  playlistgenrs /media/tv1 /media/tv2 -o ~/combined.m3u
  playlistgenrs /media/tv --interleave episodes --block-size 5
  playlistgenrs /media/tv --interleave seasons
  playlistgenrs /media/tv --relative --repeat";

#[derive(Parser, Debug)]
#[command(
    name = "playlistgenrs",
    version,
    about = "Interleave TV show episodes from subdirectories into one M3U playlist.",
    after_help = EPILOG
)]
struct Args {
    /// Launch the interactive text menu
    #[arg(long)]
    text: bool,

    /// One or more directories containing TV show subdirectories
    #[arg(value_name = "media_dir")]
    media_dirs: Vec<PathBuf>,

    /// Output playlist file (default: <first media_dir>/interleaved.m3u)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Write relative paths in the playlist instead of absolute paths
    #[arg(long)]
    relative: bool,

    /// Cycle shorter shows back to episode 1 instead of leaving gaps. Playlist length = longest show × number of shows.
    #[arg(long)]
    repeat: bool,

    /// 'episodes': N consecutive episodes per show (set N with --block-size); 'seasons': one full season per show before switching.
    #[arg(
        long = "interleave",
        value_name = "MODE",
        value_enum,
        default_value = "none"
    )]
    interleave_mode: Mode,

    /// Episodes per show per round for --interleave episodes (default: 10)
    #[arg(long, value_name = "N", value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..))]
    block_size: Option<usize>,
}

/// Process entry point; returns the exit code.
pub fn main() -> i32 {
    if std::env::args_os().len() == 1 {
        return launch_gui();
    }
    let args = Args::parse();
    if args.text {
        let stdin = std::io::stdin();
        return crate::tui::run_menu(&mut stdin.lock(), &mut std::io::stdout());
    }
    if args.media_dirs.is_empty() {
        Args::command()
            .error(
                ErrorKind::MissingRequiredArgument,
                "the following arguments are required: media_dir",
            )
            .exit();
    }
    run(args)
}

fn run(args: Args) -> i32 {
    let dirs: Vec<PathBuf> = args.media_dirs.iter().map(|d| resolve(d)).collect();
    if let Some(d) = dirs.iter().find(|d| !d.is_dir()) {
        eprintln!("Error: '{}' is not a directory.", d.display());
        return 1;
    }
    if args.block_size.is_some() && args.interleave_mode != Mode::Episodes {
        eprintln!(
            "Warning: --block-size has no effect when --interleave is '{}'.",
            args.interleave_mode.as_str()
        );
    }
    let output = match &args.output {
        Some(o) => resolve(o),
        None => dirs[0].join("interleaved.m3u"),
    };
    let opts = BuildOptions {
        relative: args.relative,
        repeat: args.repeat,
        mode: args.interleave_mode,
        block_size: args.block_size.unwrap_or(10),
        ..Default::default()
    };
    let report = build_playlist(&dirs, &output, &opts, &mut rand::rng());
    print_report(&report);
    if report.entries == 0 {
        return 1;
    }
    println!(
        "\nPlaylist written to: {}  ({} entries)",
        output.display(),
        report.entries
    );
    0
}

/// Print a build report the way Python does: warnings and errors to stderr, slots to stdout.
pub fn print_report(r: &BuildReport) {
    for w in &r.warnings {
        eprintln!("{w}");
    }
    for l in r.summary_lines() {
        println!("{l}");
    }
    if let Some(e) = &r.error {
        eprintln!("{e}");
    }
}

#[cfg(feature = "gui")]
fn launch_gui() -> i32 {
    crate::gui::run()
}

#[cfg(not(feature = "gui"))]
fn launch_gui() -> i32 {
    eprintln!(
        "Error: this build of playlistgenrs was compiled without the GUI. \
         Use --text for the interactive menu, or pass media directories (see --help)."
    );
    1
}
