use anyhow::{bail, format_err, Error};

use proxmox_schema::const_regex;

pub fn verify_pve_volume_id_or_qm_path(s: &str) -> Result<(), Error> {
    if s == "none" || s == "cdrom" || s.starts_with('/') {
        return Ok(());
    }

    verify_volume_id(s)
}

#[rustfmt::skip]
macro_rules! DNS_NAMERE { () => (r##"([a-zA-Z0-9]([a-zA-Z0-9\-]*[a-zA-Z0-9])?)"##) }
#[rustfmt::skip]
macro_rules! DNS_RE { () => (concat!("(", DNS_NAMERE!(), "\\.)*", DNS_NAMERE!(), "$")) }

const_regex! {

pub VOLUME_ID = r##"^(?i)([a-z][a-z0-9\-\_\.]*[a-z0-9]):(.+)$"##;
//pub DNS_NAMERE = concat!("^", DNS_NAMERE!(), "$");
pub DNS_RE = concat!("^", DNS_RE!(), "$");

}

pub fn verify_volume_id(s: &str) -> Result<(), Error> {
    if VOLUME_ID.is_match(s) {
        Ok(())
    } else {
        bail!("not a valid volume id");
    }
}

pub fn verify_pve_phys_bits(s: &str) -> Result<(), Error> {
    s.parse::<u32>()
        .ok()
        .and_then(|n| (n >= 8 && n <= 64).then_some(()))
        .ok_or_else(|| format_err!("invalid number of bits"))
}

pub fn verify_ipv4(s: &str) -> Result<(), Error> {
    if pdm_api_types::common_regex::IPV4_REGEX.is_match(s) {
        Ok(())
    } else {
        bail!("not a valid IPv4 address");
    }
}

pub fn verify_ipv6(s: &str) -> Result<(), Error> {
    if pdm_api_types::common_regex::IPV6_REGEX.is_match(s) {
        Ok(())
    } else {
        bail!("not a valid IPv6 address");
    }
}

pub fn verify_ip(s: &str) -> Result<(), Error> {
    if pdm_api_types::common_regex::IP_REGEX.is_match(s) {
        Ok(())
    } else {
        bail!("not a valid IP address");
    }
}

pub fn verify_cidrv4(s: &str) -> Result<(), Error> {
    match s.find('/') {
        None => bail!("not a CIDR notation"),
        Some(pos) => {
            verify_ipv4(&s[..pos])?;
            match s[(pos + 1)..].parse::<u8>() {
                Ok(n) if n > 32 => bail!("invalid prefix length in CIDR"),
                Err(_) => bail!("not a valid CIDR notation"),
                Ok(_) => Ok(()),
            }
        }
    }
}

pub fn verify_cidrv6(s: &str) -> Result<(), Error> {
    match s.find('/') {
        None => bail!("not a CIDR notation"),
        Some(pos) => {
            verify_ipv6(&s[..pos])?;
            match s[(pos + 1)..].parse::<u8>() {
                Ok(n) if n > 128 => bail!("invalid prefix length in CIDR"),
                Err(_) => bail!("not a valid CIDR notation"),
                Ok(_) => Ok(()),
            }
        }
    }
}

pub fn verify_ipv4_config(s: &str) -> Result<(), Error> {
    if s == "dhcp" || s == "manual" {
        return Ok(());
    }
    verify_cidrv4(s)
}

pub fn verify_ipv6_config(s: &str) -> Result<(), Error> {
    if s == "dhcp" || s == "manual" || s == "auto" {
        return Ok(());
    }
    verify_cidrv6(s)
}

pub fn verify_dns_name(s: &str) -> Result<(), Error> {
    if DNS_RE.is_match(s) {
        Ok(())
    } else {
        bail!("not a valid dns name")
    }
}

pub fn verify_address(s: &str) -> Result<(), Error> {
    if DNS_RE.is_match(s) {
        return Ok(());
    }
    verify_ip(s).map_err(|_| format_err!("not a valid address"))
}