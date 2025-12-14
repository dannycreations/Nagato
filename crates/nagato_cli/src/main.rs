use nagato_cli::{ClapParser, Cli};

fn main() {
  let cli = Cli::parse();
  // I'm cloning `cli` here to retain access to its properties (specifically the file path)
  // for our custom error handling logic, even after the original `cli` object is moved
  // into the `nagato_cli::run` function. This is a key part of the new error reporting infrastructure.
  if let Err(e) = nagato_cli::run(cli.clone()) {
    eprintln!("Error: {}", e.kind);
    if let Some(line) = e.line {
      if let Some(file) = cli.file {
        eprintln!("    at {}:{}", file.to_str().unwrap(), line);
      }
    }
    std::process::exit(1);
  }
}
