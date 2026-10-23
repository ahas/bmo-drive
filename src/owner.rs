//! The owner a request acts for (`docs/api.md`, 「Owners」): the opaque key the caller hands in `Owner-Key`,
//! taken as given and compared exactly, beside the role the reader holds in `Owner-Role`, which must allow
//! the console bundle's role; and whether the key is the administrator's, whose alone the machine branch,
//! the storage and the quotas are.

use crate::console;
use crate::http::Refused;
use crate::settings::Settings;
use crate::wire::ConsoleRole;

/// The header naming the owner a request acts for.
pub const OWNER_KEY: &str = "owner-key";

/// The header naming the role the reader holds.
pub const OWNER_ROLE: &str = "owner-role";

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

  /// The owner of a key a request names, held by a reader in the role it names: refused where it names no
  /// role allowing the console bundle's, where it names no key, and where the key is longer than 200
  /// characters.
  pub fn checked(key: Option<&str>, role: Option<&str>, settings: &Settings) -> Result<Self, Refused> {
    if !role.and_then(ConsoleRole::of_name).is_some_and(|held| held.allows(console::ROLE)) {
      return Err(Refused::forbidden(format!(
        "the route needs a role allowing {}, Owner-Role",
        console::ROLE.name()
      )));
    }

    Self::keyed(key, settings)
  }

  /// The owner of a key, refused where there is none or it is longer than 200 characters.
  pub fn keyed(key: Option<&str>, settings: &Settings) -> Result<Self, Refused> {
    match key {
      Some(key) if !key.is_empty() && key.chars().count() <= KEY_MOST => Ok(Self::of_key(key, settings)),
      Some(key) if !key.is_empty() => {
        Err(Refused::bad_request("the owner key is longer than 200 characters"))
      }
      _ => Err(Refused::forbidden("the route needs an owner key, Owner-Key")),
    }
  }

  /// Nothing where the owner is the administrator; to any other, what is the administrator's alone is
  /// answered as if it did not stand, saying `error`.
  pub fn administrator_only(&self, error: &str) -> Result<(), Refused> {
    if self.administrator { Ok(()) } else { Err(Refused::not_found(error)) }
  }
}

impl ConsoleRole {
  /// The role a header names, where it names one.
  pub fn of_name(name: &str) -> Option<Self> {
    match name {
      "administrator" => Some(Self::Administrator),
      "member" => Some(Self::Member),
      _ => None,
    }
  }

  /// The role as a header names it.
  pub fn name(self) -> &'static str {
    match self {
      Self::Administrator => "administrator",
      Self::Member => "member",
    }
  }

  /// Whether a reader holding this role may open what needs `needed`: an administrator holds every role a
  /// member holds.
  pub fn allows(self, needed: Self) -> bool {
    self == Self::Administrator || needed == Self::Member
  }
}
