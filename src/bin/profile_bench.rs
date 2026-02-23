use clap::Parser;
use kopy::config::{Cli, ScanMode, StorageProfile};
use kopy::Config;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "profile_bench")]
#[command(about = "Benchmark kopy storage profiles (auto/hdd/ssd) on a dataset")]
struct BenchArgs {
    /// Source directory to copy from.
    source: PathBuf,
    /// Scratch root where temporary destination directories are created.
    scratch_root: PathBuf,
    /// Transfer worker threads (0 = auto).
    #[arg(long, default_value_t = 0)]
    threads: usize,
    /// Scan strategy used during benchmark runs.
    #[arg(long, value_enum, default_value_t = ScanMode::Auto)]
    scan_mode: ScanMode,
    /// Keep generated benchmark destination directories.
    #[arg(long, default_value_t = false)]
    keep: bool,
}

fn main() -> anyhow::Result<()> {
    let args = BenchArgs::parse();
    fs::create_dir_all(&args.scratch_root)?;

    let profiles = [
        StorageProfile::Auto,
        StorageProfile::Hdd,
        StorageProfile::Ssd,
    ];
    let mut results: Vec<(StorageProfile, u128)> = Vec::new();

    for profile in profiles {
        let dest = args
            .scratch_root
            .join(format!(".kopy_profile_bench_{profile:?}"));
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        fs::create_dir_all(&dest)?;

        let cli = Cli {
            source: args.source.clone(),
            destination: dest.clone(),
            dry_run: false,
            checksum: false,
            delete: false,
            delete_permanent: false,
            exclude: vec![],
            include: vec![],
            scan_mode: args.scan_mode,
            threads: args.threads,
            storage_profile: profile,
            large_transfer_threshold: None,
        };
        let config = Config::try_from(cli)?;

        let start = Instant::now();
        kopy::commands::sync::run(config)?;
        let elapsed = start.elapsed().as_millis();
        println!("{profile:?}: {elapsed} ms");
        results.push((profile, elapsed));

        if !args.keep {
            fs::remove_dir_all(&dest)?;
        }
    }

    if let Some((best_profile, best_ms)) = results.iter().min_by_key(|(_, ms)| *ms) {
        println!("Recommended profile for this dataset: {best_profile:?} ({best_ms} ms)");
    }

    Ok(())
}
