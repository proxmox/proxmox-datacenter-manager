use serde::{Deserialize, Serialize};

use proxmox_schema::api;

/// All available languages in Proxmox. Taken from proxmox-i18n repository.
/// pt_BR, zh_CN, and zh_TW use the same case in the translation files.
// TODO: auto-generate from available translations
#[api]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Translation {
    /// Arabic
    Ar,
    /// Catalan
    Ca,
    /// Danish
    Da,
    /// German
    De,
    /// English
    En,
    /// Spanish
    Es,
    /// Euskera
    Eu,
    /// Persian (Farsi)
    Fa,
    /// French
    Fr,
    /// Galician
    Gl,
    /// Hebrew
    He,
    /// Hungarian
    Hu,
    /// Italian
    It,
    /// Japanese
    Ja,
    /// Korean
    Kr,
    /// Norwegian (Bokmal)
    Nb,
    /// Dutch
    Nl,
    /// Norwegian (Nynorsk)
    Nn,
    /// Polish
    Pl,
    /// Portuguese (Brazil)
    #[serde(rename = "pt_BR")]
    PtBr,
    /// Russian
    Ru,
    /// Slovenian
    Sl,
    /// Swedish
    Sv,
    /// Turkish
    Tr,
    /// Chinese (simplified)
    #[serde(rename = "zh_CN")]
    ZhCn,
    /// Chinese (traditional)
    #[serde(rename = "zh_TW")]
    ZhTw,
}
