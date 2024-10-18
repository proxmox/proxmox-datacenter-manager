use serde::{Deserialize, Serialize};

use proxmox_schema::{api, Updater};

use crate::{
    Translation, EMAIL_SCHEMA, HTTP_PROXY_SCHEMA, OPENSSL_CIPHERS_TLS_1_2_SCHEMA,
    OPENSSL_CIPHERS_TLS_1_3_SCHEMA,
};

#[api(
    properties: {
       "http-proxy": {
            schema: HTTP_PROXY_SCHEMA,
            optional: true,
        },
        "email-from": {
            schema: EMAIL_SCHEMA,
            optional: true,
        },
        "ciphers-tls-1.3": {
            schema: OPENSSL_CIPHERS_TLS_1_3_SCHEMA,
            optional: true,
        },
        "ciphers-tls-1.2": {
            schema: OPENSSL_CIPHERS_TLS_1_2_SCHEMA,
            optional: true,
        },
        "default-lang" : {
            schema: Translation::API_SCHEMA,
            optional: true,
        },
    },
)]
#[derive(Deserialize, Serialize, Updater)]
#[serde(rename_all = "kebab-case")]
/// Node specific configuration.
pub struct NodeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_proxy: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_from: Option<String>,

    /// List of TLS ciphers for TLS 1.3 that will be used by the proxy. (Proxy has to be restarted for changes to take effect)
    #[serde(skip_serializing_if = "Option::is_none", rename = "ciphers-tls-1.3")]
    pub ciphers_tls_1_3: Option<String>,

    /// List of TLS ciphers for TLS <= 1.2 that will be used by the proxy. (Proxy has to be restarted for changes to take effect)
    #[serde(skip_serializing_if = "Option::is_none", rename = "ciphers-tls-1.2")]
    pub ciphers_tls_1_2: Option<String>,

    /// Default language used in the GUI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_lang: Option<String>,
}
