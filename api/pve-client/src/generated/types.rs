use std::collections::HashMap;

use proxmox_schema::api;

#[api]
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum IsRunning {
    #[serde(rename = "running")]
    /// running.
    Running,
    #[serde(rename = "stopped")]
    /// stopped.
    Stopped,
}
serde_plain::derive_display_from_serialize!(IsRunning);
serde_plain::derive_fromstr_from_deserialize!(IsRunning);

#[api]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct VersionResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console: Option<VersionResponseConsole>,

    /// The current Proxmox VE point release in `x.y` format.
    pub release: String,

    /// The short git revision from which this version was build.
    pub repoid: String,

    /// The full pve-manager package version of this node.
    pub version: String,
}

#[api]
/// The default console viewer to use.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum VersionResponseConsole {
    #[serde(rename = "applet")]
    /// applet.
    Applet,
    #[serde(rename = "vv")]
    /// vv.
    Vv,
    #[serde(rename = "html5")]
    /// html5.
    Html5,
    #[serde(rename = "xtermjs")]
    /// xtermjs.
    Xtermjs,
}
serde_plain::derive_display_from_serialize!(VersionResponseConsole);
serde_plain::derive_fromstr_from_deserialize!(VersionResponseConsole);
