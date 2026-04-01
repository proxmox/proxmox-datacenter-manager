use serde::{Deserialize, Serialize};

use proxmox_acme_api::AcmeAccountName;
use proxmox_schema::{api, ApiStringFormat, ArraySchema, Schema, StringSchema};

use crate::EMAIL_SCHEMA;

pub const ACME_CONTACT_LIST_SCHEMA: Schema =
    StringSchema::new("List of email addresses, comma seperated.")
        .format(&ApiStringFormat::PropertyString(
            &ArraySchema::new("Contact list.", &EMAIL_SCHEMA).schema(),
        ))
        .schema();

#[api(
    properties: {
        name: {
            type: AcmeAccountName,
            optional: true,
        },
        contact: {
            schema: ACME_CONTACT_LIST_SCHEMA
        },
        tos_url: {
            type: String,
            optional: true,
        },
        directory: {
            type: String,
            optional: true,
        },
        eab_kid: {
            type: String,
            optional: true,
        },
        eab_hmac_key: {
            type: String,
            optional: true,
        }
    },
)]
#[derive(Serialize, Deserialize)]
/// ACME account registration properties.
pub struct AcmeRegistrationParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<AcmeAccountName>,

    pub contact: String,

    /// URL of CA TermsOfService - setting this indicates agreement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tos_url: Option<String>,

    /// The ACME Directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,

    /// Key Identifier for External Account Binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eab_kid: Option<String>,

    /// HMAC key for External Account Binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eab_hmac_key: Option<String>,
}
