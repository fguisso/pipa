use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "pipa", version, about = "pipa CLI")]
struct Cli {}

fn main() -> Result<()> {
    let _ = Cli::parse();
    println!("pipa CLI — scaffold");
    Ok(())
}
