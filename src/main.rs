use clap::Parser;
use kopy::config::Cli;
use kopy::Config;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Convert CLI args to Config - this validates immediately
    let config = Config::try_from(cli)?;

    println!("kopy v{}", kopy::VERSION);
    kopy::commands::sync::run(config)?;

    Ok(())
}
