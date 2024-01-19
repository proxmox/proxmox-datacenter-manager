//! Client environment to query login data.

use std::io::{self, Write};

use anyhow::{bail, format_err, Error};
use http::Uri;
use openssl::x509;

use proxmox_client::TfaChallenge;

use crate::XDG;

mod fingerprint_cache;
use fingerprint_cache::FingerprintCache;

macro_rules! xdg_path {
    ($text:literal) => {
        concat!("proxmox-datacenter-client/", $text)
    };
}

pub(crate) use xdg_path;

/// Supported types.
const TOTP: u8 = 1;
const RECOVERY: u8 = 2;
const YUBICO: u8 = 4;
const WEBAUTHN: u8 = 8;

/// We store the last used user id in here.
const USERID_CACHE_PATH: &str = xdg_path!("userid");
const FINGERPRINT_CACHE_PATH: &str = xdg_path!("fingerprints");
const CURRENT_SERVER_CACHE_PATH: &str = xdg_path!("current-server");

pub struct Env {
    pub server: Option<String>,
    pub userid: Option<String>,
    pub fingerprint_cache: FingerprintCache,
}

impl Env {
    pub fn need_userid(&self) -> Result<&str, Error> {
        self.userid
            .as_deref()
            .ok_or_else(|| format_err!("no userid to login with was specified"))
    }

    pub fn from_args<A>(args: A) -> Result<(Self, Vec<String>), Error>
    where
        A: Iterator<Item = String>,
    {
        let mut this = Self {
            server: None,
            userid: None,
            fingerprint_cache: FingerprintCache::new(),
        };

        if let Some(file) = XDG.find_cache_file(FINGERPRINT_CACHE_PATH) {
            let cache = std::fs::read_to_string(file)?;
            this.fingerprint_cache.load(&cache)?;
        }

        if let Some(file) = XDG.find_cache_file(CURRENT_SERVER_CACHE_PATH) {
            let server = std::fs::read_to_string(&file)?;
            if let Err(err) = this.set_server(server.trim()) {
                eprintln!("bad server in cache file ({file:?}): {err}");
            }
        }

        let args = this.parse_arguments(args)?;
        Ok((this, args))
    }

    /// Parse the client parameters out and return the remaining parameters.
    pub fn parse_arguments<A>(&mut self, mut args: A) -> Result<Vec<String>, Error>
    where
        A: Iterator<Item = String>,
    {
        let mut out = Vec::new();
        out.push(
            args.next()
                .ok_or_else(|| format_err!("no parameters provided"))?,
        );

        while let Some(arg) = args.next() {
            if let Some(server) = arg.strip_prefix("--server=") {
                self.update_server(server)?;
            } else if arg == "--server" {
                self.update_server(
                    &args
                        .next()
                        .ok_or_else(|| format_err!("missing value for `--server` parameter"))?,
                )?;
            } else if arg == "--" {
                // break without including the `--` separator
                break;
            } else {
                // first unrecognized parameter: include it in the output
                out.push(arg);
                break;
            }
        }
        out.extend(args);

        Ok(out)
    }

    fn update_server(&mut self, server: &str) -> Result<(), Error> {
        self.set_server(server)?;

        let write_file = |path| {
            let mut file = std::fs::File::create(path)?;
            file.write_all(server.as_bytes())?;
            file.write_all(b"\n")?;
            Ok(())
        };

        match XDG
            .place_cache_file(CURRENT_SERVER_CACHE_PATH)
            .and_then(write_file)
        {
            Ok(()) => (),
            Err(err) => eprintln!("failed to store current server in cache: {}", err),
        }

        Ok(())
    }

    fn set_server(&mut self, server: &str) -> Result<(), Error> {
        let uri: Uri = server.parse()?;
        let parts = uri.into_parts();

        if let Some(scheme) = parts.scheme {
            if scheme == http::uri::Scheme::HTTP {
                log::warn!("ignoring 'http://' scheme, using https instead");
            } else if scheme != http::uri::Scheme::HTTPS {
                bail!("invalid address scheme: '{scheme}'");
            }
        }

        if let Some(paq) = parts.path_and_query {
            if !paq.path().is_empty() && paq.path() != "/" {
                // TODO:
                bail!("unsupported url (path currently ignored)");
            }
            if paq.query().is_some() {
                bail!("unsupported url (should not contain a query)");
            }
        }

        let authority = parts
            .authority
            .ok_or_else(|| format_err!("invalid url (missing authority): {server:?}"))?;

        // authority doesn't actually give us proper access to its components -_-
        let host = authority.host();
        let user_at = authority.as_str().strip_suffix(host).unwrap();
        let user = user_at
            .strip_suffix('@')
            .ok_or_else(|| format_err!("missing username in url"))?;

        self.server = Some(host.to_string());
        self.userid = Some(user.to_string());

        Ok(())
    }

    pub fn verify_cert(&self, chain: &mut x509::X509StoreContextRef) -> Result<bool, Error> {
        let result = match self.server.as_deref() {
            Some(server) => self.fingerprint_cache.verify(server, chain)?,
            None => return Ok(false),
        };

        if result.modified {
            let data = self.fingerprint_cache.write()?;
            match XDG
                .place_cache_file(FINGERPRINT_CACHE_PATH)
                .and_then(|path| std::fs::write(path, data.as_bytes()))
            {
                Ok(()) => (),
                Err(err) => eprintln!("failed to store userid in cache: {}", err),
            }
        }

        Ok(result.valid)
    }
}

impl Env {
    fn ticket_path(api_url: &Uri, userid: &str) -> String {
        format!(
            xdg_path!("{}/ticket-{}"),
            api_url.to_string().replace('/', "+"),
            userid
        )
    }

    pub fn remember_userid(userid: &str) {
        match XDG
            .place_cache_file(USERID_CACHE_PATH)
            .and_then(|path| std::fs::write(path, userid.as_bytes()))
        {
            Ok(()) => (),
            Err(err) => eprintln!("failed to store userid in cache: {}", err),
        }
    }
}

impl Env {
    pub fn query_userid(&self, _api_url: &http::Uri) -> Result<String, Error> {
        if let Some(userid) = &self.userid {
            return Ok(userid.clone());
        }

        if let Some(path) = XDG.find_cache_file(USERID_CACHE_PATH) {
            let userid = std::fs::read_to_string(path)?;
            let userid = userid.trim_start().trim_end();
            if !userid.is_empty() {
                println!("Using userid {userid:?}");
                return Ok(userid.to_owned());
            }
        }

        print!("Userid: ");
        io::stdout().flush()?;
        let mut userid = String::new();
        io::stdin().read_line(&mut userid)?;
        while userid.ends_with('\n') {
            userid.pop();
        }

        Env::remember_userid(&userid);

        Ok(userid)
    }

    pub fn query_password(&self, _api_url: &http::Uri, _userid: &str) -> Result<String, Error> {
        let password = proxmox_sys::linux::tty::read_password("Password: ")?;
        Ok(String::from_utf8(password)?)
    }

    pub fn query_second_factor(
        &self,
        api_url: &Uri,
        userid: &str,
        challenge: &TfaChallenge,
    ) -> Result<String, Error> {
        println!(
            "A second factor is required to authenticate as {:?} on {:?}",
            userid, api_url
        );

        #[rustfmt::skip]
        let available = (challenge.totp as u8 * TOTP)
            | (challenge.recovery.is_available() as u8 * RECOVERY)
            | (challenge.yubico as u8 * YUBICO)
            | (challenge.webauthn.is_some() as u8 * WEBAUTHN);

        if available == 0 {
            bail!("no supported 2nd factors available");
        }

        let mut response = String::new();
        let tfa_type = if available.count_ones() > 1 {
            let mut types = Vec::new();
            loop {
                response.clear();
                types.clear();

                println!("Available supported 2nd factors:");
                if 0 != (available & TOTP) {
                    println!("[{}] totp", types.len());
                    types.push(TOTP);
                }
                if 0 != (available & WEBAUTHN) {
                    println!("[{}] webauthn", types.len());
                    types.push(WEBAUTHN);
                }
                if 0 != (available & YUBICO) {
                    println!("[{}] Yubico OTP", types.len());
                    types.push(YUBICO);
                }
                if 0 != (available & RECOVERY) {
                    println!("[{}] recovery", types.len());
                    types.push(RECOVERY);
                }
                print!("Choose 2nd factor to use: ");
                std::io::stdout().flush()?;

                std::io::stdin().read_line(&mut response)?;
                let response = response.trim_end_matches('\n');
                match response.parse::<usize>() {
                    Ok(num) if num < types.len() => break types[num],
                    _ => println!("invalid choice"),
                }
            }
        } else {
            available
        };
        response.clear();

        let prefix;
        if tfa_type == TOTP {
            print!("Please type in your TOTP code: ");
            prefix = "totp";
        } else if tfa_type == YUBICO {
            print!("Please push the button on your Yubico OTP device: ");
            prefix = "yubico";
        } else if tfa_type == RECOVERY {
            print!("Please type in one of the available recovery codes: ");
            prefix = "recovery";
        } else if tfa_type == WEBAUTHN {
            let mut fido = crate::fido::Fido::new();
            println!("Please push the button on your FIDO2 device.");
            // Unwrap: WEBAUTHN is not in the available list if it's not Some.
            let response =
                fido.get_assertion(challenge.webauthn.as_ref().unwrap(), &api_url.to_string())?;
            return Ok(format!("webauthn:{response}"));
        } else {
            // not possible
            bail!("unsupported tfa type selected");
        }

        std::io::stdout().flush()?;
        std::io::stdin().read_line(&mut response)?;
        Ok(format!(
            "{}:{}",
            prefix,
            response
                .trim_start_matches(|b: char| b.is_ascii_whitespace())
                .trim_end_matches(|b: char| b.is_ascii_whitespace())
        ))
    }

    pub fn store_ticket(&self, api_url: &Uri, userid: &str, ticket: &[u8]) -> Result<(), Error> {
        let path = XDG.place_cache_file(Env::ticket_path(api_url, userid))?;
        std::fs::write(path, ticket).map_err(Error::from)
    }

    pub fn load_ticket(&self, api_url: &Uri, userid: &str) -> Result<Option<Vec<u8>>, Error> {
        Ok(
            match XDG.find_cache_file(Env::ticket_path(api_url, userid)) {
                Some(path) => Some(std::fs::read(path)?),
                None => None,
            },
        )
    }

    /*
    fn sleep(
        time: std::time::Duration,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>, Error> {
        Ok(Box::pin(tokio::time::sleep(time)))
    }
    */
}
