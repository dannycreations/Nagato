use std::process;

use nagato_cli::{ClapParser, Cli};

fn main() {
  let cli = Cli::parse();
  if let Err(e) = nagato_cli::run(&cli) {
    eprintln!("Error: {}", e.kind);
    if let Some(line) = e.line {
      let location = cli.files.first().map_or_else(
        || "<stdin>".to_string(),
        |f| f.to_string_lossy().to_string(),
      );
      eprintln!("    at {}:{}", location, line);
    }
    process::exit(1);
  }
}
