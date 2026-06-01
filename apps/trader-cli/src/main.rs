use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "trader")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    Migrate,
    Import,
    Backtest,
    Replay,
    Report,
    CheckConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => println!("initialized"),
        Command::Migrate => println!("migrated"),
        Command::Import => println!("imported"),
        Command::Backtest => println!("backtest started"),
        Command::Replay => println!("replay started"),
        Command::Report => println!("report generated"),
        Command::CheckConfig => println!("config ok"),
    }
    Ok(())
}
