use std::borrow::Cow;

use nagato_cli::{ClapParser, Cli};

fn main() {
  let cli = Cli::parse();
  if let Err(e) = nagato_cli::run(&cli) {
    eprintln!("Error: {}", e.kind);
    if let Some(line) = e.line {
      let location: Cow<str> = cli
        .files
        .first()
        .map_or_else(|| Cow::from("<stdin>"), |f| f.to_string_lossy());
      eprintln!("    at {}:{}", location, line);
    }
    std::process::exit(1);
  }
}
