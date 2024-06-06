use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::Error;

use proxmox_sys::fs::file_get_contents;
use proxmox_sys::fs::{replace_file, CreateOptions};

use proxmox_auth_api::HMACKey;

use pdm_buildcfg::configdir;

pub fn csrf_secret() -> &'static HMACKey {
    static SECRET: OnceLock<HMACKey> = OnceLock::new();

    SECRET.get_or_init(|| {
        let bytes = file_get_contents(configdir!("/auth/csrf.key")).unwrap();
        std::str::from_utf8(&bytes)
            .map_err(anyhow::Error::new)
            .and_then(HMACKey::from_base64)
            // legacy fall back to load legacy csrf secrets
            // TODO: remove once we move away from legacy token verification
            .unwrap_or_else(|_| {
                let key_as_b64 = base64::encode_config(bytes, base64::STANDARD_NO_PAD);
                HMACKey::from_base64(&key_as_b64).unwrap()
            })
    })
}

pub fn generate_csrf_key() -> Result<(), Error> {
    let path = PathBuf::from(configdir!("/auth/csrf.key"));

    if path.exists() {
        return Ok(());
    }

    let key = HMACKey::generate()?.to_base64()?;

    use nix::sys::stat::Mode;

    let api_user = pdm_config::api_user()?;

    replace_file(
        &path,
        &key.as_bytes(),
        CreateOptions::new()
            .perm(Mode::from_bits_truncate(0o0640))
            .owner(nix::unistd::ROOT)
            .group(api_user.gid),
        true,
    )?;

    Ok(())
}
