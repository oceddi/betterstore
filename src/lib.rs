

pub mod api {
    // Used to pull in generated Rust in target build folder from .proto
    tonic::include_proto!("api");
}

// Pull in modules defined in subdirs below
pub mod actor;

#[cfg(test)]
pub(crate) mod test_util {
  use std::sync::atomic::{AtomicUsize, Ordering};

  static COUNTER: AtomicUsize = AtomicUsize::new(0);

  // Fresh, not-yet-created directory path per call so tests can run in parallel.
  pub fn temp_dir(name: &str) -> String {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("betterstore_{}_{}_{}", name, std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    dir.to_str().unwrap().to_string()
  }
}
