#![feature(drain_filter)]
#![feature(slice_internals)]
#![feature(slice_concat_trait)]
#![feature(once_cell)]

mod build;
mod lit;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Interface {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build the compiler and supporting toolchain
    Build(self::build::Config),
    /// Run lit tests against the compiler
    Lit(self::lit::Config),
}

fn main() -> anyhow::Result<()> {
    let cli = Interface::parse();

    match &cli.command {
        Commands::Build(ref config) => self::build::run(config),
        Commands::Lit(ref config) => self::lit::run(config),
    }
}
