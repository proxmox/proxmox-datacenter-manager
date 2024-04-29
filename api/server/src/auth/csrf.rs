use std::path::PathBuf;

use anyhow::{bail, format_err, Error};
use lazy_static::lazy_static;
use openssl::rsa::Rsa;
use openssl::sha;

use proxmox_lang::try_block;
use proxmox_sys::fs::file_get_contents;
use proxmox_sys::fs::{replace_file, CreateOptions};

use pdm_api_types::Userid;
use pdm_buildcfg::configdir;

pub fn csrf_secret() -> &'static [u8] {
    lazy_static! {
        static ref SECRET: Vec<u8> = file_get_contents(configdir!("/auth/csrf.key")).unwrap();
    }
    &SECRET
}

pub fn assemble_csrf_prevention_token(userid: &Userid) -> String {
    let epoch = proxmox_time::epoch_i64();

    let digest = compute_csrf_secret_digest(epoch, csrf_secret(), userid);

    format!("{:08X}:{}", epoch, digest)
}

pub fn verify_csrf_prevention_token(
    userid: &Userid,
    token: &str,
    min_age: i64,
    max_age: i64,
) -> Result<i64, Error> {
    use std::collections::VecDeque;

    let mut parts: VecDeque<&str> = token.split(':').collect();

    try_block!({
        if parts.len() != 2 {
            bail!("format error - wrong number of parts.");
        }

        let timestamp = parts.pop_front().unwrap();
        let sig = parts.pop_front().unwrap();

        let ttime = i64::from_str_radix(timestamp, 16)
            .map_err(|err| format_err!("timestamp format error - {}", err))?;

        let digest = compute_csrf_secret_digest(ttime, csrf_secret(), userid);

        if digest != sig {
            bail!("invalid signature.");
        }

        let now = proxmox_time::epoch_i64();

        let age = now - ttime;
        if age < min_age {
            bail!("timestamp newer than expected.");
        }

        if age > max_age {
            bail!("timestamp too old.");
        }

        Ok(age)
    })
    .map_err(|err| format_err!("invalid csrf token - {}", err))
}

fn compute_csrf_secret_digest(timestamp: i64, secret: &[u8], userid: &Userid) -> String {
    let mut hasher = sha::Sha256::new();
    let data = format!("{:08X}:{}:", timestamp, userid);
    hasher.update(data.as_bytes());
    hasher.update(secret);

    base64::encode_config(hasher.finish(), base64::STANDARD_NO_PAD)
}

pub fn generate_csrf_key() -> Result<(), Error> {
    let path = PathBuf::from(configdir!("/auth/csrf.key"));

    if path.exists() {
        return Ok(());
    }

    let rsa = Rsa::generate(2048).unwrap();

    let pem = rsa.private_key_to_pem()?;

    use nix::sys::stat::Mode;

    let api_user = pdm_config::api_user()?;

    replace_file(
        &path,
        &pem,
        CreateOptions::new()
            .perm(Mode::from_bits_truncate(0o0640))
            .owner(nix::unistd::ROOT)
            .group(api_user.gid),
        true,
    )?;

    Ok(())
}
