use std::borrow::Cow;

use nagato_cli::{ClapParser, Cli};

fn main() {
  let cli = Cli::parse();
  if let Err(e) = nagato_cli::run(&cli) {
    eprintln!("Error: {}", e.kind);
    if let Some(line) = e.line {
      // By using `Cow`, we avoid allocating a new `String` if the path is already valid UTF-8.
      // This is a zero-cost abstraction when the data doesn't need to be owned.
      let location: Cow<str> = cli
        .file
        .as_deref()
        .map_or_else(|| Cow::from("<stdin>"), |f| f.to_string_lossy());
      eprintln!("    at {}:{}", location, line);
    }
    std::process::exit(1);
  }
}
