//! Client environment to query login data.

use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};

use anyhow::{bail, format_err, Context as _, Error};
use http::Uri;
use openssl::x509;
use serde::{Deserialize, Serialize};

use proxmox_auth_api::types::Userid;
use proxmox_client::TfaChallenge;
use proxmox_schema::api;

use crate::config::{FormatArgs, PdmConnectArgs};
use crate::XDG;

mod fingerprint_cache;
pub use fingerprint_cache::Fingerprint;
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

#[derive(Deserialize, Serialize)]
struct CurrentServer {
    host: String,
    user: Userid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
}

pub struct Env {
    pub format_args: FormatArgs,
    pub connect_args: PdmConnectArgs,
    pub fingerprint_cache: FingerprintCache,
}

impl Env {
    pub fn need_userid(&self) -> Result<&Userid, Error> {
        self.connect_args
            .user
            .as_ref()
            .ok_or_else(|| format_err!("no userid to login with was specified"))
    }

    pub fn url(&self) -> Result<String, Error> {
        Ok(format!(
            "https://{}:{}/",
            self.connect_args
                .host
                .as_deref()
                .ok_or_else(|| format_err!("no host specified"))?,
            self.connect_args.port.unwrap_or(8443)
        ))
    }

    /// The pdm url with the `user@` part used for the `current-server` file in `~/.cache`.

    pub fn new() -> Result<Self, Error> {
        let mut this = Self {
            format_args: FormatArgs::default(),
            connect_args: PdmConnectArgs::default(),
            fingerprint_cache: FingerprintCache::new(),
        };

        if let Some(file) = XDG.find_cache_file(FINGERPRINT_CACHE_PATH) {
            let cache = std::fs::read_to_string(file)?;
            this.fingerprint_cache.load(&cache)?;
        }

        Ok(this)
    }

    /// Recall from `~/.cache/current-server`, unless parameters have been set.
    pub fn recall_current_server(&mut self) -> Result<(), Error> {
        if self.connect_args.host.is_some()
            || self.connect_args.user.is_some()
            || self.connect_args.port.is_some()
        {
            return Ok(());
        }

        let Some(file) = XDG.find_cache_file(CURRENT_SERVER_CACHE_PATH) else {
            return Ok(());
        };

        let data = match std::fs::read(&file) {
            Ok(data) => data,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err.into()),
        };

        let data: CurrentServer = serde_json::from_slice(&data)?;
        self.connect_args.host = Some(data.host);
        self.connect_args.user = Some(data.user);
        self.connect_args.port = data.port;
        Ok(())
    }

    pub fn remember_current_server(&self) -> Result<(), Error> {
        let Some(host) = self.connect_args.host.clone() else {
            return Ok(());
        };
        let Some(user) = self.connect_args.user.clone() else {
            return Ok(());
        };

        let data = serde_json::to_string(&CurrentServer {
            host,
            user,
            port: self.connect_args.port,
        })?;

        let path = XDG.place_cache_file(CURRENT_SERVER_CACHE_PATH)?;
        std::fs::write(path, data.as_bytes())?;
        Ok(())
    }

    pub fn verify_cert(&self, chain: &mut x509::X509StoreContextRef) -> Result<bool, Error> {
        let result = match self.connect_args.host.as_deref() {
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
    fn ticket_path(api_url: &Uri, userid: &Userid) -> String {
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
    pub fn query_userid(&self, _api_url: &http::Uri) -> Result<Userid, Error> {
        if let Some(userid) = self.connect_args.user.clone() {
            return Ok(userid);
        }

        if let Some(path) = XDG.find_cache_file(USERID_CACHE_PATH) {
            let userid = std::fs::read_to_string(path)?;
            let userid = userid.trim_start().trim_end();
            if !userid.is_empty() {
                println!("Using userid {userid:?}");
                return userid.parse().context("invalid user id");
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

        userid.parse().context("invalid user id")
    }

    pub fn query_password(&self, api_url: &http::Uri, userid: &Userid) -> Result<String, Error> {
        if let Some(pw) = self.connect_args.get_password()? {
            return Ok(pw);
        }

        println!("Password required for user {userid} at {api_url}");
        let password = proxmox_sys::linux::tty::read_password("Password: ")?;
        Ok(String::from_utf8(password)?)
    }

    pub fn query_second_factor(
        &self,
        api_url: &Uri,
        userid: &Userid,
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
            let response = perform_fido_auth(
                api_url,
                challenge
                    .webauthn
                    .as_ref()
                    .ok_or_else(|| format_err!("received webauthn challenge without data"))?,
            )?;
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

    pub fn store_ticket(&self, api_url: &Uri, userid: &Userid, ticket: &[u8]) -> Result<(), Error> {
        let path = XDG.place_cache_file(Env::ticket_path(api_url, userid))?;
        std::fs::write(path, ticket).map_err(Error::from)
    }

    pub fn load_ticket(&self, api_url: &Uri, userid: &Userid) -> Result<Option<Vec<u8>>, Error> {
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

    pub fn use_color(&self) -> bool {
        self.format_args.color.to_bool()
    }
}

fn perform_fido_auth(
    api_url: &http::Uri,
    challenge: &webauthn_rs::proto::RequestChallengeResponse,
) -> Result<String, Error> {
    use proxmox_fido2::FidoOpt;
    use webauthn_rs::proto::UserVerificationPolicy;

    let public_key = &challenge.public_key;
    let raw_challenge: &[u8] = public_key.challenge.as_ref();
    let b64u_challenge = base64::encode_config(raw_challenge, base64::URL_SAFE_NO_PAD);
    let client_data_json = serde_json::to_string(&serde_json::json!({
        "type": "webauthn.get",
        "origin": api_url.to_string().trim_end_matches('/'),
        "challenge": b64u_challenge.as_str(),
        "clientExtensions": {},
    }))
    .expect("failed to build json string");
    let hash = openssl::sha::sha256(client_data_json.as_bytes());

    let libfido = proxmox_fido2::Lib::open()?;

    let mut first = true;
    'device: for dev_info in libfido.list_devices(None)? {
        if !std::mem::replace(&mut first, false) {
            println!("Trying next device...");
        }
        log::debug!(
            "opening FIDO2 device {manufacturer:?} {product:?} at {path:?}",
            manufacturer = dev_info.manufacturer,
            product = dev_info.product,
            path = dev_info.path,
        );
        let dev = match libfido.dev_open(&dev_info.path) {
            Ok(dev) => dev,
            Err(err) => {
                log::debug!(
                    "failed to open FIDO2 device {path:?} - {err}",
                    path = dev_info.path,
                );
                continue;
            }
        };
        let options = match dev.options() {
            Ok(o) => o,
            Err(err) => {
                log::error!(
                    "error getting device options for {path:?}: {err:?}",
                    path = dev_info.path
                );
                continue 'device;
            }
        };

        let mut assert = libfido
            .assert_new()?
            .set_relying_party(public_key.rp_id.as_str())?
            .set_user_verification_required(match public_key.user_verification {
                UserVerificationPolicy::Discouraged => {
                    if options.user_verification {
                        FidoOpt::False
                    } else {
                        FidoOpt::Omit
                    }
                }
                UserVerificationPolicy::Preferred_DO_NOT_USE => FidoOpt::Omit,
                UserVerificationPolicy::Required => FidoOpt::True,
            })?
            .set_clientdata_hash(&hash)?;
        for cred in &public_key.allow_credentials {
            assert = assert.allow_cred(cred.id.as_ref())?;
        }

        let mut pin = None;
        'with_pin: loop {
            return match dev.assert(&mut assert, pin.as_deref()) {
                Ok(assert) => finish_fido_auth(assert, client_data_json, b64u_challenge),
                Err(proxmox_fido2::Error::NoCredentials) => {
                    println!("Device did not contain the required credentials");
                    continue 'device;
                }
                Err(proxmox_fido2::Error::PinRequired) if pin.is_none() => {
                    let user_pin = proxmox_sys::linux::tty::read_password("fido2 pin: ")?;
                    pin = Some(
                        String::from_utf8(user_pin)
                            .map_err(|_| format_err!("invalid bytes in pin"))?,
                    );
                    continue 'with_pin;
                }
                Err(err) => return Err(err.into()),
            };
        }
    }

    bail!("failed to perform fido2 authentication");
}

fn finish_fido_auth(
    assert: proxmox_fido2::FidoAssertSigned<'_>,
    client_data_json: String,
    b64u_challenge: String,
) -> Result<String, Error> {
    use webauthn_rs::base64_data::Base64UrlSafeData;

    let id = assert.id()?;
    let sig = assert.signature()?;
    let auth_data = assert.auth_data()?;
    let auth_data = match serde_cbor::from_slice::<serde_cbor::Value>(auth_data)? {
        serde_cbor::Value::Bytes(bytes) => bytes,
        _ => bail!("auth data has invalid format"),
    };

    let response = webauthn_rs::proto::PublicKeyCredential {
        type_: "public-key".to_string(),
        id: base64::encode_config(id, base64::URL_SAFE_NO_PAD),
        raw_id: Base64UrlSafeData(id.to_vec()),
        extensions: None,
        response: webauthn_rs::proto::AuthenticatorAssertionResponseRaw {
            authenticator_data: Base64UrlSafeData(auth_data),
            signature: Base64UrlSafeData(sig.to_vec()),
            user_handle: None,
            client_data_json: Base64UrlSafeData(client_data_json.into_bytes()),
        },
    };

    let mut response = serde_json::to_value(response)?;
    response["response"]
        .as_object_mut()
        .unwrap()
        .remove("userHandle");
    response.as_object_mut().unwrap().remove("extensions");
    response["challenge"] = b64u_challenge.into();

    Ok(serde_json::to_string(&response)?)
}

#[api]
/// Control terminal color output.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UseColor {
    /// Never use colored output.
    #[default]
    No,
    /// Force ANSI color output.
    Always,
    /// Automatically decide whether to use colored output.
    Auto,
}

serde_plain::derive_deserialize_from_fromstr!(UseColor, "valid color formatting option");
serde_plain::derive_display_from_serialize!(UseColor);

impl std::str::FromStr for UseColor {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        Ok(match s {
            "no" => Self::No,
            "always" | "yes" | "on" => Self::Always,
            "auto" => Self::Auto,
            _ => bail!("bad argument for '--color', should be one of 'no', 'always' or 'auto'"),
        })
    }
}

pub fn complete_color(arg: &str, _param: &HashMap<String, String>) -> Vec<String> {
    ["no", "yes", "on", "always", "auto"]
        .into_iter()
        .filter(|value| value.starts_with(arg))
        .map(str::to_string)
        .collect()
}

impl UseColor {
    /// Convert to a boolean by having 'Auto' test whether `stdout` is a tty.
    pub fn to_bool(self) -> bool {
        match self {
            Self::No => false,
            Self::Always => true,
            Self::Auto => std::io::stdout().is_terminal(),
        }
    }
}
