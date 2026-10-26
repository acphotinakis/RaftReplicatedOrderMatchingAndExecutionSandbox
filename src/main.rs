mod cli;

use clap::Parser;
use raft_replicated_order_matching_and_execution_sandbox::args::CliArgs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    cli::run(CliArgs::parse().command)
}
