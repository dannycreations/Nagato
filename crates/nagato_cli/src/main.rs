use nagato_cli::{ClapParser, Cli};

fn main() {
  let cli = Cli::parse();
  // We clone `cli` to retain access to its properties (like the file path)
  // for our custom error handling, even after the original `cli` object
  // is moved into the `nagato_cli::run` function.
  if let Err(e) = nagato_cli::run(cli.clone()) {
    // The error message is now more user-friendly, stating the nature of the error clearly.
    eprintln!("Error: {}", e.kind);
    if let Some(line) = e.line {
      // The error reporting now explicitly handles stdin and gracefully falls back
      // to `to_string_lossy` for non-UTF-8 paths, preventing panics.
      let location = cli.file.map_or_else(
        || "<stdin>".to_string(),
        |f| f.to_string_lossy().into_owned(),
      );
      eprintln!("    at {}:{}", location, line);
    }
    std::process::exit(1);
  }
}
