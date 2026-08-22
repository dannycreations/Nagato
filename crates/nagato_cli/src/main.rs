use std::process::exit;

use clap::Parser;

mod cmd;

use cmd::{execute, Cli};

fn main() {
  let cli = Cli::parse();

  if cli.version {
    let version =
      option_env!("NAGATO_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    println!("{version}");
    exit(0);
  }

  if let Err(e) = execute(cli) {
    eprintln!("Error: {e}");
    exit(1);
  }
}
