use clap::Parser;
use kopy::config::Cli;
use kopy::Config;

fn main() -> anyhow::Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        return Err(anyhow::anyhow!(
            "kopy currently supports Linux only. Use a Linux environment to run this build."
        ));
    }

    let cli = Cli::parse();

    // Convert CLI args to Config - this validates immediately
    let config = Config::try_from(cli)?;

    println!("kopy v{}", kopy::VERSION);
    kopy::commands::sync::run(config)?;

    Ok(())
}
