//! FIDO2 support with libfido2

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_char, c_int, c_void};

use libc::size_t;

use anyhow::{bail, format_err, Error};

#[repr(C)]
enum FidoOpt {
    Omit,
    False,
    True,
}

#[link(name = "fido2")]
extern "C" {
    fn fido_init(flags: c_int);

    fn fido_strerr(err: c_int) -> *const c_char;

    fn fido_dev_info_new(n: size_t) -> *mut c_void;
    fn fido_dev_info_free(ptr: *mut *mut c_void, len: size_t);
    fn fido_dev_info_manifest(dl: *mut c_void, ilen: size_t, olen: *mut size_t);
    fn fido_dev_info_ptr(dl: *const c_void, index: size_t) -> DevInfoRef;
    fn fido_dev_info_path(dl: DevInfoRef) -> *const u8;

    // This is not in debian yet...
    //fn fido_dev_new_with_info(info: DevInfoRef) -> *mut c_void;
    fn fido_dev_new() -> *mut c_void;
    fn fido_dev_free(dev: *mut *mut c_void);
    // This is not in debian yet...
    // fn fido_dev_open_with_info(dev: *mut c_void) -> c_int;
    fn fido_dev_open(dev: *mut c_void, path: *const u8) -> c_int;
    fn fido_dev_close(dev: *mut c_void) -> c_int;

    fn fido_assert_new() -> *mut c_void;
    fn fido_assert_free(a: *mut *mut c_void);
    fn fido_assert_set_rp(a: *mut c_void, rpid: *const c_char) -> c_int;
    fn fido_assert_set_uv(a: *mut c_void, uv: FidoOpt) -> c_int;
    fn fido_assert_set_clientdata_hash(a: *mut c_void, data: *const u8, size: size_t) -> c_int;
    fn fido_assert_allow_cred(a: *mut c_void, data: *const u8, size: size_t) -> c_int;

    fn fido_dev_get_assert(dev: *mut c_void, a: *mut c_void, pin: *const c_char) -> c_int;

    fn fido_assert_count(a: *mut c_void) -> size_t;
    fn fido_assert_id_ptr(a: *mut c_void, idx: size_t) -> *const u8;
    fn fido_assert_id_len(a: *mut c_void, idx: size_t) -> size_t;
    //fn fido_assert_user_id_ptr(a: *mut c_void, idx: size_t) -> *const u8;
    //fn fido_assert_user_id_len(a: *mut c_void, idx: size_t) -> size_t;
    fn fido_assert_sig_ptr(a: *mut c_void, idx: size_t) -> *const u8;
    fn fido_assert_sig_len(a: *mut c_void, idx: size_t) -> size_t;
    fn fido_assert_authdata_ptr(a: *mut c_void, idx: size_t) -> *const u8;
    fn fido_assert_authdata_len(a: *mut c_void, idx: size_t) -> size_t;
    //fn fido_assert_clientdata_ptr(a: *mut c_void, idx: size_t) -> *const u8;
}

#[derive(Debug)]
enum FidoError {
    PinRequired,
    Other(&'static CStr),
}

impl fmt::Display for FidoError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FidoError::PinRequired => f.write_str("pin required"),
            FidoError::Other(msg) => write!(f, "{msg:?}"),
        }
    }
}

impl std::error::Error for FidoError {}

fn fido_err(err: c_int) -> FidoError {
    match err {
        0x36 => FidoError::PinRequired,
        err => unsafe {
            let ptr = fido_strerr(err);
            FidoError::Other(if ptr.is_null() {
                CStr::from_bytes_with_nul_unchecked(b"Success")
            } else {
                CStr::from_ptr(ptr)
            })
        },
    }
}

fn fido_result(err: c_int) -> Result<(), FidoError> {
    if err == 0 {
        Ok(())
    } else {
        Err(fido_err(err))
    }
}

/// Access to libfido2.
///
/// This is neither `Send` nor `Sync`.
pub struct Fido {
    _init: *mut (),
}

impl Default for Fido {
    fn default() -> Self {
        Self::new()
    }
}

impl Fido {
    pub fn new() -> Self {
        unsafe {
            fido_init(0);
        };

        Self { _init: &mut () }
    }

    fn device_info(&self) -> Result<DevInfo, Error> {
        DevInfo::new()
    }

    pub fn get_assertion(
        &mut self,
        challenge: &webauthn_rs::proto::RequestChallengeResponse,
        origin: &str,
    ) -> Result<String, Error> {
        let assertion = Assert::new(challenge, origin)?;

        for dev_info in self.device_info()? {
            let dev = match dev_info.open() {
                Ok(dev) => dev,
                Err(err) => {
                    eprintln!("failed to open FIDO2 device: {err}");
                    continue;
                }
            };
            match fido_result(unsafe {
                fido_dev_get_assert(dev.ptr, assertion.ptr, std::ptr::null())
            }) {
                Ok(()) => (),
                Err(err) => {
                    eprintln!("failed to get FIDO2 assertion: {err}");
                    continue;
                }
            }

            return finish_response(assertion);
        }
        bail!("failed to get FIDO2 assertion");
    }
}

fn finish_response(assertion: Assert) -> Result<String, Error> {
    let count = unsafe { fido_assert_count(assertion.ptr) };
    if count != 1 {
        bail!("unexpecteda ssertion count: {count}");
    }

    let (id, sig, authdata) = unsafe {
        (
            fido_assert_id_ptr(assertion.ptr, 0),
            //fido_assert_user_id_ptr(assertion.ptr, 0),
            fido_assert_sig_ptr(assertion.ptr, 0),
            fido_assert_authdata_ptr(assertion.ptr, 0),
        )
    };

    if id.is_null() {
        bail!("failed to get ID from assertion");
    }
    //if user_id.is_null() {
    //    bail!("failed to get user id from assertion");
    //}
    if sig.is_null() {
        bail!("failed to get signature from assertion");
    }
    if authdata.is_null() {
        bail!("failed to get auth data from assertion");
    }

    let (idlen, siglen, authdatalen) = unsafe {
        (
            fido_assert_id_len(assertion.ptr, 0),
            //fido_assert_user_id_len(assertion.ptr, 0),
            fido_assert_sig_len(assertion.ptr, 0),
            fido_assert_authdata_len(assertion.ptr, 0),
        )
    };

    #[allow(clippy::unnecessary_cast)]
    let (id, sig, authdata) = unsafe {
        (
            std::slice::from_raw_parts(id, idlen as usize),
            //std::slice::from_raw_parts(user_id, user_idlen as usize),
            std::slice::from_raw_parts(sig, siglen as usize),
            std::slice::from_raw_parts(authdata, authdatalen as usize),
        )
    };

    let authdata = match serde_cbor::from_slice::<serde_cbor::Value>(authdata)? {
        serde_cbor::Value::Bytes(bytes) => bytes,
        _ => bail!("auth data has invalid format"),
    };

    use webauthn_rs::base64_data::Base64UrlSafeData;
    let response = webauthn_rs::proto::PublicKeyCredential {
        type_: "public-key".to_string(),
        id: base64::encode_config(id, base64::URL_SAFE_NO_PAD),
        raw_id: Base64UrlSafeData(id.to_vec()),
        extensions: None,
        response: webauthn_rs::proto::AuthenticatorAssertionResponseRaw {
            authenticator_data: Base64UrlSafeData(authdata),
            signature: Base64UrlSafeData(sig.to_vec()),
            user_handle: None,
            client_data_json: Base64UrlSafeData(assertion.client_data_json.as_bytes().to_vec()),
        },
    };

    let mut response = serde_json::to_value(response)?;
    response["response"]
        .as_object_mut()
        .unwrap()
        .remove("userHandle");
    response.as_object_mut().unwrap().remove("extensions");
    response["challenge"] = assertion.challenge.clone().into();

    Ok(serde_json::to_string(&response)?)
}

struct RawDevInfo(*mut c_void, size_t);

impl Drop for RawDevInfo {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                fido_dev_info_free(&mut self.0, self.1);
            }
        }
    }
}

struct DevInfo {
    raw: RawDevInfo,
    count: usize,
    at: usize,
}

impl DevInfo {
    fn new() -> Result<Self, Error> {
        let alloc = 32;
        let raw = unsafe { fido_dev_info_new(alloc) };
        if raw.is_null() {
            bail!("failed to allocate FIDO2 device information");
        }
        let raw = RawDevInfo(raw, alloc as _);

        let mut got: size_t = 0;
        unsafe {
            fido_dev_info_manifest(raw.0, 32, &mut got);
        }

        Ok(Self {
            raw,
            count: got as usize,
            at: 0,
        })
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
struct DevInfoRef(*const c_void);

impl Iterator for DevInfo {
    type Item = DevInfoRef;

    fn next(&mut self) -> Option<Self::Item> {
        if self.at == self.count {
            return None;
        }

        let res = unsafe { fido_dev_info_ptr(self.raw.0, self.at as _) };

        if res.0.is_null() {
            return None;
        }

        self.at += 1;
        Some(res)
    }
}

impl DevInfoRef {
    fn open(self) -> Result<Device, Error> {
        let path = unsafe { fido_dev_info_path(self) };
        if path.is_null() {
            bail!("failed to get path for FIDO2 device");
        }

        /*
        let ptr = unsafe { fido_dev_new_with_info(self) };
        if ptr.is_null() {
            bail!("failed to allocate FIDO2 device from device information");
        }
        */
        let ptr = unsafe { fido_dev_new() };
        if ptr.is_null() {
            bail!("failed to allocate FIDO2 device");
        }
        let mut device = Device { ptr, opened: false };

        //fido_result(unsafe { fido_dev_open_with_info(device.ptr) })?;
        fido_result(unsafe { fido_dev_open(device.ptr, path) })?;

        device.opened = true;

        Ok(device)
    }
}

struct Device {
    ptr: *mut c_void,
    opened: bool,
}

impl Drop for Device {
    fn drop(&mut self) {
        if self.opened {
            self.opened = false;
            if let Err(err) = fido_result(unsafe { fido_dev_close(self.ptr) }) {
                eprintln!("failed to close FIDO2 device: {err}");
            }
        }

        unsafe {
            fido_dev_free(&mut self.ptr);
        }
    }
}

struct Assert {
    ptr: *mut c_void,
    challenge: String,
    client_data_json: String,
}

impl Drop for Assert {
    fn drop(&mut self) {
        unsafe {
            fido_assert_free(&mut self.ptr);
        }
    }
}

impl Assert {
    fn new(
        challenge: &webauthn_rs::proto::RequestChallengeResponse,
        origin: &str,
    ) -> Result<Self, Error> {
        let ptr = unsafe { fido_assert_new() };
        if ptr.is_null() {
            bail!("failed to allocate FIDO2 assertion");
        }

        let mut this = Self {
            ptr,
            challenge: String::new(),
            client_data_json: String::new(),
        };
        let challenge = this.apply_webauthn(challenge, origin)?;
        this.challenge = challenge;

        Ok(this)
    }

    fn apply_webauthn(
        &mut self,
        challenge: &webauthn_rs::proto::RequestChallengeResponse,
        origin: &str,
    ) -> Result<String, Error> {
        use webauthn_rs::proto::UserVerificationPolicy;

        let challenge = &challenge.public_key;

        let rpid = CString::new(challenge.rp_id.as_str())
            .map_err(|_| format_err!("invalid relying party id"))?;
        fido_result(unsafe { fido_assert_set_rp(self.ptr, rpid.as_ptr()) })
            .map_err(|err| format_err!("failed to set relying party id: {err}"))?;

        // FIDO API:
        let param = match challenge.user_verification {
            UserVerificationPolicy::Discouraged => FidoOpt::False,
            UserVerificationPolicy::Preferred_DO_NOT_USE => FidoOpt::Omit,
            UserVerificationPolicy::Required => FidoOpt::True,
        };
        fido_result(unsafe { fido_assert_set_uv(self.ptr, param) })
            .map_err(|err| format_err!("failed to set user verification policy: {err}"))?;

        for cred in &challenge.allow_credentials {
            let data: &[u8] = cred.id.as_ref();
            fido_result(unsafe {
                fido_assert_allow_cred(self.ptr, data.as_ptr(), data.len() as _)
            })
            .map_err(|err| format_err!("failed to add allowed credentials: {err}"))?;
        }

        let raw_challenge: &[u8] = challenge.challenge.as_ref();
        let b64u_challenge = base64::encode_config(raw_challenge, base64::URL_SAFE_NO_PAD);
        let client_data_json = serde_json::json!({
            "type": "webauthn.get",
            "origin": origin.trim_end_matches('/'),
            "challenge": b64u_challenge,
            "clientExtensions": {},
        });
        self.client_data_json = serde_json::to_string(&client_data_json)?;

        let hash = openssl::sha::sha256(self.client_data_json.as_bytes());

        fido_result(unsafe {
            fido_assert_set_clientdata_hash(self.ptr, hash.as_ptr(), hash.len() as _)
        })
        .map_err(|err| format_err!("failed to set authdata: {err}"))?;

        Ok(base64::encode_config(
            raw_challenge,
            base64::URL_SAFE_NO_PAD,
        ))
    }
}
