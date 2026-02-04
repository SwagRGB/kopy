use clap::Parser;
use kopy::Config;

/// kopy - Modern file synchronization tool
#[derive(Parser, Debug)]
#[command(name = "kopy")]
#[command(version, about, long_about = None)]
struct Cli {
    /// Source directory
    source: String,

    /// Destination directory
    destination: String,

    /// Perform a dry run (show what would be done without executing)
    #[arg(long, short = 'n')]
    dry_run: bool,

    /// Delete files in destination that don't exist in source
    #[arg(long)]
    delete: bool,

    /// Exclude patterns (can be specified multiple times)
    #[arg(long, short = 'e')]
    exclude: Vec<String>,

    /// Include patterns (can be specified multiple times)
    #[arg(long, short = 'i')]
    include: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    println!("kopy v{}", kopy::VERSION);
    println!("Source: {}", cli.source);
    println!("Destination: {}", cli.destination);
    println!("Dry run: {}", cli.dry_run);

    // TODO: Convert CLI args to Config and run sync
    // let config = Config::from_cli(&cli)?;
    // kopy::commands::sync::run(config)?;

    Ok(())
}
