use clap::Parser;
use kopy::config::Cli;
use kopy::Config;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Convert CLI args to Config - this validates immediately
    let config = Config::try_from(cli)?;

    println!("kopy v{}", kopy::VERSION);
    println!("Configuration validated successfully!");
    println!("  Source: {:?}", config.source);
    println!("  Destination: {:?}", config.destination);
    println!("  Dry run: {}", config.dry_run);
    println!("  Checksum mode: {}", config.checksum_mode);
    println!("  Delete mode: {:?}", config.delete_mode);
    println!("  Exclude patterns: {:?}", config.exclude_patterns);
    println!("  Include patterns: {:?}", config.include_patterns);

    // TODO: Run sync
    // kopy::commands::sync::run(config)?;

    Ok(())
}
