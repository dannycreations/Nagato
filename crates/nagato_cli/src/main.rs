use std::process;

use nagato_cli::{ClapParser, Cli};

fn main() {
  let cli = Cli::parse();
  if let Err(e) = nagato_cli::run(&cli) {
    eprintln!("Error: {e}");
    process::exit(1);
  }
}
