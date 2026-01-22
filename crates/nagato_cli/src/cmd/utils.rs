use std::path::{Path, PathBuf};

pub fn get_unique_path(dir: &Path, name: &str) -> PathBuf {
  let mut path = dir.join(name);
  if !path.exists() {
    return path;
  }

  // Identify the stem and the full extension chain by locating the first period in the filename.
  // This ensures that for compound extensions like '.trim.patch', the counter is inserted before the first period.
  let (stem, extension) = match name.find('.') {
    Some(index) if index > 0 => (&name[..index], &name[index..]),
    _ => (name, ""),
  };

  let mut counter = 1;
  loop {
    let new_name = format!("{}-{}{}", stem, counter, extension);
    path = dir.join(new_name);
    if !path.exists() {
      return path;
    }
    counter += 1;
  }
}
