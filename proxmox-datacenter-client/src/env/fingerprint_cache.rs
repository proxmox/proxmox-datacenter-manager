use std::collections::HashMap;
use std::io::Write;
use std::sync::RwLock;

use anyhow::{bail, format_err, Error};
use openssl::hash::MessageDigest;
use openssl::x509::X509StoreContextRef;

pub struct FingerprintCache {
    pub interactive: bool,

    entries: RwLock<HashMap<String, [u8; 32]>>,
}

pub struct VerifyResult {
    /// The certificate was accepted.
    pub valid: bool,

    /// Cache was modified and the file needs to be stored.
    pub modified: bool,
}

impl VerifyResult {
    fn unmodified(valid: bool) -> Self {
        Self {
            valid,
            modified: false,
        }
    }
}

impl Default for FingerprintCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FingerprintCache {
    pub fn new() -> Self {
        Self {
            interactive: unsafe { libc::isatty(0) == 1 },
            entries: Default::default(),
        }
    }

    /// Verify a certificate.
    pub fn verify(
        &self,
        hostname: &str,
        chain: &mut X509StoreContextRef,
    ) -> Result<VerifyResult, Error> {
        let cert = chain
            .current_cert()
            .ok_or_else(|| format_err!("no certificate in chain?"))?;

        let fp = match cert.digest(MessageDigest::sha256()) {
            Err(err) => bail!("error calculating certificate fingerprint: {err}"),
            Ok(fp) => fp,
        };

        if let Some(stored_fp) = self.entries.read().unwrap().get(hostname) {
            return Ok(VerifyResult::unmodified(*stored_fp == *fp));
        }

        let fp =
            <[u8; 32]>::try_from(&*fp).map_err(|_| format_err!("unexpected fingerprint length"))?;

        if !self.interactive {
            return Ok(VerifyResult::unmodified(false));
        }

        println!("Certificate SHA256 fingerprint: {}", fp_string(&fp));

        let mut stdout = std::io::stdout();
        stdout.write_all(b"Do you want to trust this certificate? [No/yes/once] ")?;
        stdout.flush()?;
        let reply = match std::io::stdin().lines().next() {
            None => return Ok(VerifyResult::unmodified(false)),
            Some(line) => line?.to_ascii_lowercase(),
        };

        if reply == "once" {
            return Ok(VerifyResult::unmodified(true));
        }

        if !(reply == "y" || reply == "yes") {
            return Ok(VerifyResult::unmodified(false));
        }

        self.entries
            .write()
            .unwrap()
            .insert(hostname.to_string(), fp);

        Ok(VerifyResult {
            valid: true,
            modified: true,
        })
    }

    pub fn write(&self) -> Result<String, Error> {
        use std::fmt::Write;

        let mut out = String::new();
        for (host, fp) in self.entries.read().unwrap().iter() {
            writeln!(out, "{host} {}", hex::encode(fp))?;
        }

        Ok(out)
    }

    pub fn load(&mut self, cache_data: &str) -> Result<(), Error> {
        let mut entries = self.entries.write().unwrap();
        entries.clear();

        for (lineno, line) in cache_data.lines().enumerate() {
            let lineno = lineno + 1; // start counting lines at 1

            let line = line.trim_start();
            if line.starts_with('#') {
                continue;
            }

            let mut parts = line.trim_end().split_ascii_whitespace();
            let host = parts
                .next()
                .ok_or_else(|| format_err!("empty ({lineno}) in fingerprint cache"))?;

            let fp = parts
                .next()
                .ok_or_else(|| format_err!("bad line ({lineno}) in fingerprint cache"))?;

            let fp = hex::decode(fp.as_bytes())
                .map_err(drop)
                .and_then(|fp| <[u8; 32]>::try_from(&fp[..]).map_err(drop))
                .map_err(|_| format_err!("bad fingerprint in fingerprint cache (line {lineno})"))?;

            entries.insert(host.to_string(), fp);
        }

        Ok(())
    }
}

fn fp_string(fp: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for b in fp {
        if !out.is_empty() {
            out.push(':');
        }
        let _ = write!(out, "{b:02x}");
    }
    out
}
