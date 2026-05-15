use std::process::exit;

use nagato_cli::{execute, Cli, Parser};

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
