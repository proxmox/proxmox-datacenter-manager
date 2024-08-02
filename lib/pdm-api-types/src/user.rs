use serde::{Deserialize, Serialize};

use proxmox_schema::api;

#[api]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Properties of users which can be unset/deleted.
pub enum DeletableUserProperty {
    /// Clear the comment field.
    Comment,
    /// Reset the enabled state to its default of being enabled.
    Enable,
    /// Clear the expiration date.
    Expire,
    /// Clear the first name.
    Firstname,
    /// Clear the last name.
    Lastname,
    /// Clear the associated email address.
    Email,
}

serde_plain::derive_display_from_serialize!(DeletableUserProperty);
serde_plain::derive_fromstr_from_deserialize!(DeletableUserProperty);
