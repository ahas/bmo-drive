//! Where this machine refuses a change (`docs/files.md`, 「Protected locations」): the settings' list for a
//! machine read by a POSIX shell, under this process's own home, with the settings' further folders of this
//! machine and whatever its places add beside them.

use std::fs;
use std::path::{Path, PathBuf};

use crate::settings::{PosixProtect, Protect};

/// The protected locations of this machine, each as it really stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Guard {
  /// Each protected with everything inside it, and refused as anything that holds it.
  within: Vec<PathBuf>,
  /// Each whose own entries are protected.
  top: Vec<PathBuf>,
}

/// A location as it really stands where it does, as written where it does not.
fn really(path: &Path) -> PathBuf {
  fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

impl Guard {
  /// The guard of a machine whose home is `home`, read by `policy`, with `besides` protected beside it.
  pub fn new(policy: &PosixProtect, home: &Path, besides: &[PathBuf]) -> Self {
    let named = policy.home_named.iter().flat_map(|named| {
      let under = home.join(&named.under);
      let starting = named.starting.clone();

      fs::read_dir(&under)
        .into_iter()
        .flatten()
        .flatten()
        .filter(move |one| one.file_name().to_string_lossy().starts_with(&starting))
        .map(|one| one.path())
    });
    let within = policy
      .system
      .iter()
      .map(PathBuf::from)
      .chain(policy.home.iter().map(|one| home.join(one)))
      .chain(named)
      .chain(besides.iter().cloned())
      .map(|one| really(&one))
      .collect();

    Self {
      within,
      top: policy.top.iter().map(|one| really(Path::new(one))).collect(),
    }
  }

  /// This process's own machine: the settings' lists under its home, its further folders, and `besides`.
  pub fn this_machine(protect: &Protect, besides: &[PathBuf]) -> Self {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let extra: Vec<PathBuf> = protect.local.extra.iter().chain(besides).cloned().collect();

    Self::new(&protect.posix, &home, &extra)
  }

  /// Whether a change landing on `real` — a location as it really stands, its own last name kept
  /// where it is a link — is refused.
  pub fn refuses(&self, real: &Path) -> bool {
    real.parent().is_none()
      || self.top.iter().any(|top| real.parent() == Some(top.as_path()))
      || self.within.iter().any(|one| real.starts_with(one) || one.starts_with(real))
  }
}
