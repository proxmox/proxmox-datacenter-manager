//! Macros used by the client.

macro_rules! xdg_path {
    ($text:literal) => {
        concat!("proxmox-datacenter-client/", $text)
    };
}
