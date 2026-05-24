use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "gapes", version, about = "gapes CLI")]
struct Cli {}

fn main() -> Result<()> {
    let _ = Cli::parse();
    println!("gapes CLI — scaffold");
    Ok(())
}
