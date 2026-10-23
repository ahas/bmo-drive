//! The owner a request acts for (`docs/api.md`, 「Owners」): the opaque key the caller hands in `Owner-Key`,
//! taken as given and compared exactly, and whether it is the administrator's, whose alone the machine branch,
//! the storage and the quotas are.

use crate::http::Refused;
use crate::settings::Settings;

/// The header naming the owner a request acts for.
pub const OWNER_KEY: &str = "owner-key";

// The longest key the service takes, in characters.
const KEY_MOST: usize = 200;

/// One owner, by its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
  pub key: String,
  /// Whether the key is the settings' administrator's.
  pub administrator: bool,
}

impl Owner {
  /// The owner of `key`, judged against the settings' administrator; an empty setting makes nobody it.
  pub fn of_key(key: impl Into<String>, settings: &Settings) -> Self {
    let key = key.into();
    let named = &settings.owners.administrator;

    Self {
      administrator: !named.is_empty() && key == *named,
      key,
    }
  }

  /// The owner of a key a request names, refused where it names none or one longer than 200 characters.
  pub fn checked(key: Option<&str>, settings: &Settings) -> Result<Self, Refused> {
    match key {
      Some(key) if !key.is_empty() && key.chars().count() <= KEY_MOST => Ok(Self::of_key(key, settings)),
      Some(key) if !key.is_empty() => {
        Err(Refused::bad_request("the owner key is longer than 200 characters"))
      }
      _ => Err(Refused::bad_request("the route needs an owner key, Owner-Key")),
    }
  }

  /// Nothing where the owner is the administrator; to any other, what is the administrator's alone is
  /// answered as if it did not stand, saying `error`.
  pub fn administrator_only(&self, error: &str) -> Result<(), Refused> {
    if self.administrator { Ok(()) } else { Err(Refused::not_found(error)) }
  }
}
