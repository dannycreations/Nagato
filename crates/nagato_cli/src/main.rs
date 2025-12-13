fn main() {
  if let Err(e) = nagato_cli::run() {
    eprintln!("Error: {}", e);
    std::process::exit(1);
  }
}
