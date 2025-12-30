use std::process;

use nagato_cli::{ClapParser, Cli};

fn main() {
  let cli = Cli::parse();

  if cli.version {
    let version =
      option_env!("NAGATO_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    println!("{version}",);
    process::exit(0);
  }

  if let Err(e) = nagato_cli::run(&cli) {
    eprintln!("Error: {e}");
    process::exit(1);
  }
}
