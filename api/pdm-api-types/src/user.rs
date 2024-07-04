use serde::{Deserialize, Serialize};

use proxmox_schema::{api, BooleanSchema, IntegerSchema, Schema, StringSchema, Updater};

use super::userid::{Authid, Userid, PROXMOX_TOKEN_ID_SCHEMA};
use super::{SINGLE_LINE_COMMENT_FORMAT, SINGLE_LINE_COMMENT_SCHEMA};

// FIXME: copied from PBS -> unify!

pub const ENABLE_USER_SCHEMA: Schema = BooleanSchema::new(
    "Enable the account (default). You can set this to '0' to disable the account.",
)
.default(true)
.schema();

pub const EXPIRE_USER_SCHEMA: Schema = IntegerSchema::new(
    "Account expiration date (seconds since epoch). '0' means no expiration date.",
)
.default(0)
.minimum(0)
.schema();

pub const FIRST_NAME_SCHEMA: Schema = StringSchema::new("First name.")
    .format(&SINGLE_LINE_COMMENT_FORMAT)
    .min_length(2)
    .max_length(64)
    .schema();

pub const LAST_NAME_SCHEMA: Schema = StringSchema::new("Last name.")
    .format(&SINGLE_LINE_COMMENT_FORMAT)
    .min_length(2)
    .max_length(64)
    .schema();

pub const EMAIL_SCHEMA: Schema = StringSchema::new("E-Mail Address.")
    .format(&SINGLE_LINE_COMMENT_FORMAT)
    .min_length(2)
    .max_length(64)
    .schema();

#[api(
    properties: {
        userid: {
            type: Userid,
        },
        comment: {
            optional: true,
            schema: SINGLE_LINE_COMMENT_SCHEMA,
        },
        enable: {
            optional: true,
            schema: ENABLE_USER_SCHEMA,
        },
        expire: {
            optional: true,
            schema: EXPIRE_USER_SCHEMA,
        },
        firstname: {
            optional: true,
            schema: FIRST_NAME_SCHEMA,
        },
        lastname: {
            schema: LAST_NAME_SCHEMA,
            optional: true,
         },
        email: {
            schema: EMAIL_SCHEMA,
            optional: true,
        },
        tokens: {
            type: Array,
            optional: true,
            description: "List of user's API tokens.",
            items: {
                type: ApiToken
            },
        },
    }
)]
#[derive(Serialize, Deserialize, Clone, PartialEq)]
/// User properties with added list of ApiTokens
pub struct UserWithTokens {
    pub userid: Userid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firstname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lastname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tokens: Vec<ApiToken>,
}

impl UserWithTokens {
    pub fn empty_from_user(user: User) -> Self {
        Self {
            userid: user.userid,
            comment: user.comment,
            enable: user.enable,
            expire: user.expire,
            firstname: user.firstname,
            lastname: user.lastname,
            email: user.email,
            tokens: Vec::new(),
        }
    }

    pub fn into_user(self) -> User {
        User {
            userid: self.userid,
            comment: self.comment,
            enable: self.enable,
            expire: self.expire,
            firstname: self.firstname,
            lastname: self.lastname,
            email: self.email,
        }
    }
}

#[api(
    properties: {
        tokenid: {
            schema: PROXMOX_TOKEN_ID_SCHEMA,
        },
        comment: {
            optional: true,
            schema: SINGLE_LINE_COMMENT_SCHEMA,
        },
        enable: {
            optional: true,
            schema: ENABLE_USER_SCHEMA,
        },
        expire: {
            optional: true,
            schema: EXPIRE_USER_SCHEMA,
        },
    }
)]
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
/// ApiToken properties.
pub struct ApiToken {
    pub tokenid: Authid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire: Option<i64>,
}

impl ApiToken {
    pub fn is_active(&self) -> bool {
        if !self.enable.unwrap_or(true) {
            return false;
        }
        if let Some(expire) = self.expire {
            let now = proxmox_time::epoch_i64();
            if expire > 0 && expire <= now {
                return false;
            }
        }
        true
    }
}

#[api(
    properties: {
        userid: {
            type: Userid,
        },
        comment: {
            optional: true,
            schema: SINGLE_LINE_COMMENT_SCHEMA,
        },
        enable: {
            optional: true,
            schema: ENABLE_USER_SCHEMA,
        },
        expire: {
            optional: true,
            schema: EXPIRE_USER_SCHEMA,
        },
        firstname: {
            optional: true,
            schema: FIRST_NAME_SCHEMA,
        },
        lastname: {
            schema: LAST_NAME_SCHEMA,
            optional: true,
         },
        email: {
            schema: EMAIL_SCHEMA,
            optional: true,
        },
    }
)]
#[derive(Serialize, Deserialize, Updater)]
/// User properties.
pub struct User {
    #[updater(skip)]
    pub userid: Userid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firstname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lastname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

impl User {
    pub fn is_active(&self) -> bool {
        if !self.enable.unwrap_or(true) {
            return false;
        }
        if let Some(expire) = self.expire {
            let now = proxmox_time::epoch_i64();
            if expire > 0 && expire <= now {
                return false;
            }
        }
        true
    }
}

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
