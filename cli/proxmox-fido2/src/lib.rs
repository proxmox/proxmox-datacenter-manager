use std::error::Error as StdError;
use std::ffi::{CStr, CString, OsStr};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use libc::c_int;

#[derive(Debug)]
#[non_exhaustive]
pub enum OpenError {
    MissingLibrary,
    MissingFunction(&'static CStr),
}

impl StdError for OpenError {}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::MissingLibrary => f.write_str("failed to find libfido2"),
            Self::MissingFunction(fun) => write!(f, "missing symbol '{fun:?}' in libfido2"),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    PinRequired,
    UnsupportedAlgorithm,
    NoCredentials,
    Other(String),
}

impl StdError for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::PinRequired => f.write_str("pin required"),
            Self::UnsupportedAlgorithm => f.write_str("unsupported algorithm"),
            Self::NoCredentials => f.write_str("no credentials"),
            Self::Other(err) => f.write_str(err),
        }
    }
}

fn result_msg(lib: &Lib, res: libc::c_int, what: &'static str) -> Result<(), Error> {
    match res {
        0 => Ok(()),
        0x26 => Err(Error::UnsupportedAlgorithm),
        0x2e => Err(Error::NoCredentials),
        0x36 => Err(Error::PinRequired),
        other => Err(Error::Other(format!(
            "{what}: {:?}",
            unsafe { CStr::from_ptr((lib.fido_strerr)(other)) }.to_string_lossy()
        ))),
    }
}

macro_rules! format_err {
    ($($fmt:tt)*) => {{ Error::Other(format!($($fmt)*)) }};
}
macro_rules! bail {
    ($($fmt:tt)*) => {{ return Err(format_err!($($fmt)*)); }};
}

struct Dl(*const libc::c_void);

unsafe impl Send for Dl {}
unsafe impl Sync for Dl {}

impl Drop for Dl {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self.0 as _);
        }
    }
}

impl Dl {
    fn get<F: Sized>(&self, name: &'static CStr) -> Result<F, OpenError> {
        let sym = unsafe { libc::dlsym(self.0 as _, name.as_ptr()) };

        if sym.is_null() {
            Err(OpenError::MissingFunction(name))
        } else {
            Ok(unsafe { std::mem::transmute_copy(&sym) })
        }
    }
}

pub struct Lib {
    _lib: Dl,
    fido_init: extern "C" fn(c_int),
    fido_strerr: extern "C" fn(c_int) -> *const i8,

    fido_dev_new: extern "C" fn() -> *mut libc::c_void,
    fido_dev_free: extern "C" fn(&mut *mut libc::c_void),
    fido_dev_open: extern "C" fn(*mut libc::c_void, dev: *const i8) -> libc::c_int,
    fido_dev_close: extern "C" fn(*mut libc::c_void),
    fido_dev_is_fido2: extern "C" fn(*mut libc::c_void) -> libc::c_int,
    fido_dev_get_cbor_info: extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> libc::c_int,
    fido_dev_make_cred:
        extern "C" fn(*mut libc::c_void, *mut libc::c_void, *const i8) -> libc::c_int,
    fido_dev_get_assert:
        extern "C" fn(*mut libc::c_void, *mut libc::c_void, *const i8) -> libc::c_int,

    fido_dev_info_new: extern "C" fn(libc::size_t) -> *mut libc::c_void,
    fido_dev_info_free: extern "C" fn(&mut *mut libc::c_void, libc::size_t),
    fido_dev_info_manifest:
        extern "C" fn(*mut libc::c_void, libc::size_t, *mut libc::size_t) -> libc::c_int,
    fido_dev_info_ptr: extern "C" fn(*mut libc::c_void, libc::size_t) -> *mut libc::c_void,
    fido_dev_info_manufacturer_string: extern "C" fn(*mut libc::c_void) -> *const i8,
    fido_dev_info_path: extern "C" fn(*mut libc::c_void) -> *const i8,
    fido_dev_info_product_string: extern "C" fn(*mut libc::c_void) -> *const i8,

    fido_cbor_info_new: extern "C" fn() -> *mut libc::c_void,
    fido_cbor_info_free: extern "C" fn(&mut *mut libc::c_void),
    fido_cbor_info_extensions_ptr: extern "C" fn(*mut libc::c_void) -> *const *const i8,
    fido_cbor_info_extensions_len: extern "C" fn(*mut libc::c_void) -> libc::size_t,
    fido_cbor_info_options_name_ptr: extern "C" fn(*mut libc::c_void) -> *const *const i8,
    fido_cbor_info_options_value_ptr: extern "C" fn(*mut libc::c_void) -> *const i8,
    fido_cbor_info_options_len: extern "C" fn(*mut libc::c_void) -> libc::size_t,

    fido_cred_new: extern "C" fn() -> *mut libc::c_void,
    fido_cred_free: extern "C" fn(&mut *mut libc::c_void),
    fido_cred_exclude: extern "C" fn(*mut libc::c_void, *const u8, libc::size_t) -> libc::c_int,
    fido_cred_set_extensions: extern "C" fn(*mut libc::c_void, libc::c_int) -> libc::c_int,
    fido_cred_set_rp: extern "C" fn(*mut libc::c_void, *const i8, *const i8) -> libc::c_int,
    fido_cred_set_fmt: extern "C" fn(*mut libc::c_void, *const i8) -> libc::c_int,
    fido_cred_set_type: extern "C" fn(*mut libc::c_void, libc::c_int) -> libc::c_int,
    fido_cred_set_user: extern "C" fn(
        *mut libc::c_void,
        *const u8,
        libc::size_t,
        *const i8,
        *const i8,
        *const i8,
    ) -> libc::c_int,
    fido_cred_set_clientdata_hash:
        extern "C" fn(*mut libc::c_void, *const u8, libc::size_t) -> libc::c_int,
    fido_cred_set_rk: extern "C" fn(*mut libc::c_void, FidoOpt) -> libc::c_int,
    fido_cred_set_uv: extern "C" fn(*mut libc::c_void, FidoOpt) -> libc::c_int,
    fido_cred_set_prot: extern "C" fn(*mut libc::c_void, libc::c_int) -> libc::c_int,
    fido_cred_id_ptr: extern "C" fn(*mut libc::c_void) -> *const u8,
    fido_cred_id_len: extern "C" fn(*mut libc::c_void) -> libc::size_t,
    fido_cred_sig_ptr: extern "C" fn(*mut libc::c_void, libc::size_t) -> *const u8,
    fido_cred_sig_len: extern "C" fn(*mut libc::c_void, libc::size_t) -> libc::size_t,
    fido_cred_authdata_ptr: extern "C" fn(*mut libc::c_void, libc::size_t) -> *const u8,
    fido_cred_authdata_len: extern "C" fn(*mut libc::c_void, libc::size_t) -> libc::size_t,
    fido_cred_x5c_ptr: extern "C" fn(*mut libc::c_void, libc::size_t) -> *const u8,
    fido_cred_x5c_len: extern "C" fn(*mut libc::c_void, libc::size_t) -> libc::size_t,

    fido_assert_new: extern "C" fn() -> *mut libc::c_void,
    fido_assert_free: extern "C" fn(&mut *mut libc::c_void),
    fido_assert_set_extensions: extern "C" fn(*mut libc::c_void, libc::c_int) -> libc::c_int,
    fido_assert_set_hmac_salt:
        extern "C" fn(*mut libc::c_void, *const u8, libc::size_t) -> libc::c_int,
    fido_assert_set_rp: extern "C" fn(*mut libc::c_void, *const i8) -> libc::c_int,
    fido_assert_set_clientdata_hash:
        extern "C" fn(*mut libc::c_void, *const u8, libc::size_t) -> libc::c_int,
    fido_assert_allow_cred:
        extern "C" fn(*mut libc::c_void, *const u8, libc::size_t) -> libc::c_int,
    fido_assert_set_up: extern "C" fn(*mut libc::c_void, FidoOpt) -> libc::c_int,
    fido_assert_set_uv: extern "C" fn(*mut libc::c_void, FidoOpt) -> libc::c_int,
    fido_assert_hmac_secret_ptr: extern "C" fn(*mut libc::c_void, libc::size_t) -> *const u8,
    fido_assert_hmac_secret_len: extern "C" fn(*mut libc::c_void, libc::size_t) -> libc::size_t,
    fido_assert_id_ptr: extern "C" fn(*mut libc::c_void, libc::size_t) -> *const u8,
    fido_assert_id_len: extern "C" fn(*mut libc::c_void, libc::size_t) -> libc::size_t,
    fido_assert_sig_ptr: extern "C" fn(*mut libc::c_void, libc::size_t) -> *const u8,
    fido_assert_sig_len: extern "C" fn(*mut libc::c_void, libc::size_t) -> libc::size_t,
    fido_assert_authdata_ptr: extern "C" fn(*mut libc::c_void, libc::size_t) -> *const u8,
    fido_assert_authdata_len: extern "C" fn(*mut libc::c_void, libc::size_t) -> libc::size_t,
}

/// Some library options can be set to a boolean value or be explicitly omitted.
#[repr(C)]
pub enum FidoOpt {
    /// Omit an option (leave up to the token).
    Omit,
    /// Disable an option explicitly.
    False,
    /// Enable an option explicitly.
    True,
}

static LIB: Mutex<Option<Arc<Lib>>> = Mutex::new(None);

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum CredentialProtection {
    UVOptional = 1,
    UVOptionalWithId = 2,
    UVRequired = 3,
}

impl Lib {
    pub fn open() -> Result<Arc<Self>, OpenError> {
        let mut singleton = LIB.lock().unwrap();
        if let Some(ref lib) = *singleton {
            return Ok(Arc::clone(lib));
        }

        let lib = unsafe { libc::dlopen(b"libfido2.so.1\0".as_ptr() as _, libc::RTLD_NOW) };
        if lib.is_null() {
            return Err(OpenError::MissingLibrary);
        }

        let lib = Dl(lib);

        let this = Arc::new(Self {
            fido_init: lib.get(c"fido_init")?,
            fido_strerr: lib.get(c"fido_strerr")?,
            fido_dev_new: lib.get(c"fido_dev_new")?,
            fido_dev_free: lib.get(c"fido_dev_free")?,
            fido_dev_open: lib.get(c"fido_dev_open")?,
            fido_dev_close: lib.get(c"fido_dev_close")?,
            fido_dev_is_fido2: lib.get(c"fido_dev_is_fido2")?,
            fido_dev_get_cbor_info: lib.get(c"fido_dev_get_cbor_info")?,
            fido_dev_make_cred: lib.get(c"fido_dev_make_cred")?,
            fido_dev_get_assert: lib.get(c"fido_dev_get_assert")?,

            fido_dev_info_new: lib.get(c"fido_dev_info_new")?,
            fido_dev_info_free: lib.get(c"fido_dev_info_free")?,
            fido_dev_info_manifest: lib.get(c"fido_dev_info_manifest")?,
            fido_dev_info_ptr: lib.get(c"fido_dev_info_ptr")?,
            fido_dev_info_manufacturer_string: lib.get(c"fido_dev_info_manufacturer_string")?,
            fido_dev_info_path: lib.get(c"fido_dev_info_path")?,
            fido_dev_info_product_string: lib.get(c"fido_dev_info_product_string")?,

            fido_cbor_info_new: lib.get(c"fido_cbor_info_new")?,
            fido_cbor_info_free: lib.get(c"fido_cbor_info_free")?,
            fido_cbor_info_extensions_ptr: lib.get(c"fido_cbor_info_extensions_ptr")?,
            fido_cbor_info_extensions_len: lib.get(c"fido_cbor_info_extensions_len")?,
            fido_cbor_info_options_name_ptr: lib.get(c"fido_cbor_info_options_name_ptr")?,
            fido_cbor_info_options_value_ptr: lib.get(c"fido_cbor_info_options_value_ptr")?,
            fido_cbor_info_options_len: lib.get(c"fido_cbor_info_options_len")?,

            fido_cred_new: lib.get(c"fido_cred_new")?,
            fido_cred_free: lib.get(c"fido_cred_free")?,
            fido_cred_exclude: lib.get(c"fido_cred_exclude")?,
            fido_cred_set_extensions: lib.get(c"fido_cred_set_extensions")?,
            fido_cred_set_rp: lib.get(c"fido_cred_set_rp")?,
            fido_cred_set_fmt: lib.get(c"fido_cred_set_fmt")?,
            fido_cred_set_type: lib.get(c"fido_cred_set_type")?,
            fido_cred_set_user: lib.get(c"fido_cred_set_user")?,
            fido_cred_set_clientdata_hash: lib.get(c"fido_cred_set_clientdata_hash")?,
            fido_cred_set_rk: lib.get(c"fido_cred_set_rk")?,
            fido_cred_set_uv: lib.get(c"fido_cred_set_uv")?,
            fido_cred_set_prot: lib.get(c"fido_cred_set_prot")?,
            fido_cred_id_ptr: lib.get(c"fido_cred_id_ptr")?,
            fido_cred_id_len: lib.get(c"fido_cred_id_len")?,
            fido_cred_sig_ptr: lib.get(c"fido_cred_sig_ptr")?,
            fido_cred_sig_len: lib.get(c"fido_cred_sig_len")?,
            fido_cred_authdata_ptr: lib.get(c"fido_cred_authdata_ptr")?,
            fido_cred_authdata_len: lib.get(c"fido_cred_authdata_len")?,
            fido_cred_x5c_ptr: lib.get(c"fido_cred_x5c_ptr")?,
            fido_cred_x5c_len: lib.get(c"fido_cred_x5c_len")?,

            fido_assert_new: lib.get(c"fido_assert_new")?,
            fido_assert_free: lib.get(c"fido_assert_free")?,
            fido_assert_set_extensions: lib.get(c"fido_assert_set_extensions")?,
            fido_assert_set_hmac_salt: lib.get(c"fido_assert_set_hmac_salt")?,
            fido_assert_set_rp: lib.get(c"fido_assert_set_rp")?,
            fido_assert_set_clientdata_hash: lib.get(c"fido_assert_set_clientdata_hash")?,
            fido_assert_allow_cred: lib.get(c"fido_assert_allow_cred")?,
            fido_assert_set_up: lib.get(c"fido_assert_set_up")?,
            fido_assert_set_uv: lib.get(c"fido_assert_set_uv")?,
            fido_assert_hmac_secret_ptr: lib.get(c"fido_assert_hmac_secret_ptr")?,
            fido_assert_hmac_secret_len: lib.get(c"fido_assert_hmac_secret_len")?,
            fido_assert_id_ptr: lib.get(c"fido_assert_id_ptr")?,
            fido_assert_id_len: lib.get(c"fido_assert_id_len")?,
            fido_assert_sig_ptr: lib.get(c"fido_assert_sig_ptr")?,
            fido_assert_sig_len: lib.get(c"fido_assert_sig_len")?,
            fido_assert_authdata_ptr: lib.get(c"fido_assert_authdata_ptr")?,
            fido_assert_authdata_len: lib.get(c"fido_assert_authdata_len")?,

            _lib: lib,
        });

        (this.fido_init)(0);

        *singleton = Some(Arc::clone(&this));

        Ok(this)
    }

    fn dev_new(self: &Arc<Self>) -> Result<FidoDev, Error> {
        let dev = (self.fido_dev_new)();
        if dev.is_null() {
            bail!("failed to create new fido2 device");
        }

        Ok(FidoDev {
            lib: Arc::clone(self),
            dev,
        })
    }

    pub fn dev_open(self: &Arc<Self>, path: &Path) -> Result<FidoDev, Error> {
        let dev = self.dev_new()?;
        dev.open(path)?;
        Ok(dev)
    }

    pub fn cred_new(self: &Arc<Self>) -> Result<FidoCred, Error> {
        let cred = (self.fido_cred_new)();
        if cred.is_null() {
            bail!("failed to create new fido2 credentials");
        }

        FidoCred {
            lib: Arc::clone(self),
            cred,
        }
        .set_format("packed")
    }

    pub fn assert_new(self: &Arc<Self>) -> Result<FidoAssert, Error> {
        let assert = (self.fido_assert_new)();
        if assert.is_null() {
            bail!("failed to create new fido2 assertion");
        }

        Ok(FidoAssert {
            lib: Arc::clone(self),
            assert,
        })
    }

    pub fn list_devices(
        self: &Arc<Self>,
        count: Option<usize>,
    ) -> Result<Vec<DeviceDescription>, Error> {
        let count = count.unwrap_or(64);

        let info = (self.fido_dev_info_new)(count);
        if info.is_null() {
            bail!("failed to query fido2 devices");
        }

        let info = FidoInfo {
            lib: Arc::clone(self),
            info,
            count,
            found: 0,
            at: 0,
        }
        .manifest()?;

        let mut out = Vec::with_capacity(info.found);
        for entry in info {
            out.push(entry?);
        }

        Ok(out)
    }

    /// Open the first fido2 capable device.
    pub fn dev_open_any(self: &Arc<Self>) -> Result<Option<FidoDev>, Error> {
        for entry in self.list_devices(None)? {
            let device = match self.dev_open(&entry.path) {
                Ok(d) => d,
                Err(err) => {
                    log::error!("failed to open fido2 device {:?}: {err}", entry.path);
                    continue;
                }
            };

            if device.is_fido2() {
                return Ok(Some(device));
            }
        }

        Ok(None)
    }
}

struct FidoInfo {
    lib: Arc<Lib>,
    info: *mut libc::c_void,
    count: libc::size_t,
    found: libc::size_t,
    at: libc::size_t,
}

impl Drop for FidoInfo {
    fn drop(&mut self) {
        (self.lib.fido_dev_info_free)(&mut self.info, self.count);
    }
}

impl FidoInfo {
    fn manifest(mut self) -> Result<Self, Error> {
        let mut found = 0;
        result_msg(
            &self.lib,
            (self.lib.fido_dev_info_manifest)(self.info, self.count, &mut found),
            "failed to list devices",
        )?;
        self.found = found;
        Ok(self)
    }
}

impl Iterator for FidoInfo {
    type Item = Result<DeviceDescription, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        use std::os::unix::ffi::OsStrExt;

        if self.at >= self.found {
            return None;
        }

        let ptr = (self.lib.fido_dev_info_ptr)(self.info, self.at);
        if ptr.is_null() {
            return Some(Err(format_err!("failed to query device info {}", self.at)));
        }

        let manufacturer = (self.lib.fido_dev_info_manufacturer_string)(ptr);
        if manufacturer.is_null() {
            return Some(Err(format_err!(
                "failed to query manufacturer string for device {}",
                self.at
            )));
        }
        let product = (self.lib.fido_dev_info_product_string)(ptr);
        if product.is_null() {
            return Some(Err(format_err!(
                "failed to query product string for device {}",
                self.at
            )));
        }
        let path = (self.lib.fido_dev_info_path)(ptr);
        if path.is_null() {
            return Some(Err(format_err!(
                "failed to query path for device {}",
                self.at
            )));
        }

        let (manufacturer, product, path) = unsafe {
            (
                CStr::from_ptr(manufacturer).to_string_lossy().to_string(),
                CStr::from_ptr(product).to_string_lossy().to_string(),
                CStr::from_ptr(path),
            )
        };

        let path = PathBuf::from(OsStr::from_bytes(path.to_bytes()));

        self.at += 1;

        Some(Ok(DeviceDescription {
            manufacturer,
            product,
            path,
        }))
    }
}

#[derive(Debug)]
pub struct DeviceDescription {
    pub manufacturer: String,
    pub product: String,
    pub path: PathBuf,
}

pub struct FidoDev {
    lib: Arc<Lib>,
    dev: *mut libc::c_void,
}

unsafe impl Send for FidoDev {}
unsafe impl Sync for FidoDev {}

impl Drop for FidoDev {
    fn drop(&mut self) {
        (self.lib.fido_dev_close)(self.dev);
        (self.lib.fido_dev_free)(&mut self.dev);
    }
}

impl FidoDev {
    pub fn open(&self, device: &Path) -> Result<(), Error> {
        use std::os::unix::ffi::OsStrExt;

        let device = device.as_os_str().as_bytes();
        let mut cdev = Vec::with_capacity(device.len() + 1);
        cdev.extend(device);
        cdev.push(0);

        result_msg(
            &self.lib,
            (self.lib.fido_dev_open)(self.dev, cdev.as_ptr() as _),
            "failed to open device",
        )?;

        Ok(())
    }

    pub fn is_fido2(&self) -> bool {
        (self.lib.fido_dev_is_fido2)(self.dev) != 0
    }

    pub fn make_cred(&self, cred: &FidoCred, pin: Option<&str>) -> Result<(), Error> {
        let pin_cstr;
        let pin = match pin {
            Some(pin) => {
                pin_cstr = CString::new(pin).map_err(|_| format_err!("invalid bytes in pin"))?;
                pin_cstr.as_ptr()
            }
            None => std::ptr::null(),
        };

        result_msg(
            &self.lib,
            (self.lib.fido_dev_make_cred)(self.dev, cred.cred, pin),
            "failed to make credentials",
        )
    }

    pub fn assert(&self, assert: &FidoAssert, pin: Option<&str>) -> Result<(), Error> {
        let pin_cstr;
        let pin = match pin {
            Some(pin) => {
                pin_cstr = CString::new(pin).map_err(|_| format_err!("invalid bytes in pin"))?;
                pin_cstr.as_ptr()
            }
            None => std::ptr::null(),
        };

        result_msg(
            &self.lib,
            (self.lib.fido_dev_get_assert)(self.dev, assert.assert, pin),
            "failed to get assertion",
        )
    }

    pub fn options(&self) -> Result<DeviceOptions, Error> {
        let info = (self.lib.fido_cbor_info_new)();
        if info.is_null() {
            bail!("failed to alloate fido cbor info");
        }

        let info = CborInfo {
            lib: Arc::clone(&self.lib),
            info,
        };

        result_msg(
            &self.lib,
            (self.lib.fido_dev_get_cbor_info)(self.dev, info.info),
            "failed to get cbor info for device",
        )?;

        let mut options = DeviceOptions::default();

        let extensions = (self.lib.fido_cbor_info_extensions_ptr)(info.info);
        if !extensions.is_null() {
            let len = (self.lib.fido_cbor_info_extensions_len)(info.info);
            let extensions = unsafe { std::slice::from_raw_parts(extensions as *const &i8, len) };
            for ext in extensions {
                let name = unsafe { CStr::from_ptr(*ext) };
                if name.to_bytes() == b"hmac-secret" {
                    options.hmac_secret = true;
                    break;
                }
            }
        }

        let option_names = (self.lib.fido_cbor_info_options_name_ptr)(info.info);
        if !option_names.is_null() {
            let option_values = (self.lib.fido_cbor_info_options_value_ptr)(info.info);
            if option_values.is_null() {
                bail!("failed to query available options");
            }

            let len = (self.lib.fido_cbor_info_options_len)(info.info);

            let option_names =
                unsafe { std::slice::from_raw_parts(option_names as *const &i8, len) };
            let option_values = unsafe { std::slice::from_raw_parts(option_values, len) };

            for (name, value) in std::iter::zip(option_names, option_values) {
                let name = unsafe { CStr::from_ptr(*name) };
                match name.to_bytes() {
                    b"rk" => options.resident_key = *value != 0,
                    b"clientPin" => options.client_pin = *value != 0,
                    b"up" => options.user_presence = *value != 0,
                    b"uv" => options.user_verification = *value != 0,
                    _ => continue,
                }
            }
        }

        Ok(options)
    }
}

pub struct CborInfo {
    lib: Arc<Lib>,
    info: *mut libc::c_void,
}

impl Drop for CborInfo {
    fn drop(&mut self) {
        (self.lib.fido_cbor_info_free)(&mut self.info);
    }
}

#[derive(Debug, Default)]
pub struct DeviceOptions {
    pub hmac_secret: bool,
    pub resident_key: bool,
    pub client_pin: bool,
    pub user_presence: bool,
    pub user_verification: bool,
}

pub struct FidoCred {
    lib: Arc<Lib>,
    cred: *mut libc::c_void,
}

unsafe impl Send for FidoCred {}
unsafe impl Sync for FidoCred {}

impl Drop for FidoCred {
    fn drop(&mut self) {
        (self.lib.fido_cred_free)(&mut self.cred);
    }
}

impl FidoCred {
    /// Set FIDO_EXT_HMAC_SECRET.
    pub fn set_hmac_extension(self) -> Result<Self, Error> {
        if (self.lib.fido_cred_set_extensions)(self.cred, 0x01) != 0 {
            bail!("failed to enable hmac extension");
        }
        Ok(self)
    }

    /// Set the relying party information.
    pub fn set_relying_party(self, id: &str, name: &str) -> Result<Self, Error> {
        let id = CString::new(id).map_err(|_| format_err!("invalid bytes in relying party id"))?;
        let name =
            CString::new(name).map_err(|_| format_err!("invalid bytes in relying party name"))?;
        if (self.lib.fido_cred_set_rp)(self.cred, id.as_ptr(), name.as_ptr()) != 0 {
            bail!("failed to set relying part information");
        }
        Ok(self)
    }

    /// Set the format.
    /// NOTE: We do not want to support U2F currently, so we don't publicly expose this.
    fn set_format(self, kind: &str) -> Result<Self, Error> {
        let kind =
            CString::new(kind).map_err(|_| format_err!("invalid bytes in format specification"))?;
        if (self.lib.fido_cred_set_fmt)(self.cred, kind.as_ptr()) != 0 {
            bail!("failed to set fido format");
        }
        Ok(self)
    }

    /// Set the COSE type to use for the credentials.
    pub fn set_cose_type(self, ty: libc::c_int) -> Result<Self, Error> {
        result_msg(
            &self.lib,
            (self.lib.fido_cred_set_type)(self.cred, ty),
            "failed to set cerdentials algorithm",
        )?;
        Ok(self)
    }

    /// Use COSE_ES256 algorithm.
    pub fn set_cose_es256(self) -> Result<Self, Error> {
        self.set_cose_type(-7)
    }

    /// Set userid information.
    pub fn set_userid(
        self,
        user_id: &[u8],
        user_name: Option<&str>,
        display_name: Option<&str>,
        icon: Option<&str>,
    ) -> Result<Self, Error> {
        let user_name_cstr;
        let user_name = match user_name {
            Some(user_name) => {
                user_name_cstr = CString::new(user_name)
                    .map_err(|_| format_err!("invalid bytes in user name"))?;
                user_name_cstr.as_ptr()
            }
            None => std::ptr::null(),
        };

        let display_name_cstr;
        let display_name = match display_name {
            Some(display_name) => {
                display_name_cstr = CString::new(display_name)
                    .map_err(|_| format_err!("invalid bytes in display name"))?;
                display_name_cstr.as_ptr()
            }
            None => std::ptr::null(),
        };

        let icon_cstr;
        let icon = match icon {
            Some(icon) => {
                icon_cstr = CString::new(icon).map_err(|_| format_err!("invalid bytes in icon"))?;
                icon_cstr.as_ptr()
            }
            None => std::ptr::null(),
        };

        if (self.lib.fido_cred_set_user)(
            self.cred,
            user_id.as_ptr(),
            user_id.len(),
            user_name,
            display_name,
            icon,
        ) != 0
        {
            bail!("failed to set user information");
        }
        Ok(self)
    }

    /// Clear the client data hash.
    pub fn clear_clientdata_hash(self) -> Result<Self, Error> {
        let hash = [0u8; 32];
        if (self.lib.fido_cred_set_clientdata_hash)(self.cred, hash.as_ptr(), hash.len()) != 0 {
            bail!("failed to set clear clientdata hash");
        }
        Ok(self)
    }

    /// Set the client data hash.
    pub fn set_clientdata_hash(self, hash: &[u8; 32]) -> Result<Self, Error> {
        if (self.lib.fido_cred_set_clientdata_hash)(self.cred, hash.as_ptr(), hash.len()) != 0 {
            bail!("failed to set clientdata hash");
        }
        Ok(self)
    }

    /// Exclude/disallow a specific client id.
    pub fn exclude_cred(self, cid: &[u8]) -> Result<Self, Error> {
        if (self.lib.fido_cred_exclude)(self.cred, cid.as_ptr(), cid.len()) != 0 {
            bail!("failed to declare excluded client id");
        }
        Ok(self)
    }

    /// Disable resident key.
    pub fn set_resident_key(self, opt: FidoOpt) -> Result<Self, Error> {
        if (self.lib.fido_cred_set_rk)(self.cred, opt) != 0 {
            bail!("failed to set resident key requirement");
        }
        Ok(self)
    }

    /// Disable resident key.
    pub fn disable_resident_key(self) -> Result<Self, Error> {
        if (self.lib.fido_cred_set_rk)(self.cred, FidoOpt::False) != 0 {
            bail!("failed to set disable resident key");
        }
        Ok(self)
    }

    /// Disable user verification.
    pub fn set_user_verification(self, opt: FidoOpt) -> Result<Self, Error> {
        if (self.lib.fido_cred_set_uv)(self.cred, opt) != 0 {
            bail!("failed to set user verification policy");
        }
        Ok(self)
    }

    /// Set credential protection policy.
    pub fn set_protection(self, prot: Option<CredentialProtection>) -> Result<Self, Error> {
        if (self.lib.fido_cred_set_prot)(self.cred, prot.map(|p| p as i32).unwrap_or(0)) != 0 {
            bail!("failed to set credential protection policy");
        }
        Ok(self)
    }

    /// Get the current ID.
    pub fn id(&self) -> Result<&[u8], Error> {
        let cid = (self.lib.fido_cred_id_ptr)(self.cred);
        if cid.is_null() {
            bail!("failed to get credential id pointer");
        }
        let len = (self.lib.fido_cred_id_len)(self.cred);
        Ok(unsafe { std::slice::from_raw_parts(cid, len) })
    }

    /// Get the current signature.
    /// Usable after creating webauthn credentials.
    pub fn signature(&self) -> Result<&[u8], Error> {
        let sig = (self.lib.fido_cred_sig_ptr)(self.cred, 0);
        if sig.is_null() {
            bail!("failed to get credentials signature pointer");
        }
        let len = (self.lib.fido_cred_sig_len)(self.cred, 0);
        Ok(unsafe { std::slice::from_raw_parts(sig, len) })
    }

    /// Get the current auth data.
    /// Usable after creating webauthn credentials.
    pub fn auth_data(&self) -> Result<&[u8], Error> {
        let authdata = (self.lib.fido_cred_authdata_ptr)(self.cred, 0);
        if authdata.is_null() {
            bail!("failed to get credentials auth data pointer");
        }
        let len = (self.lib.fido_cred_authdata_len)(self.cred, 0);
        Ok(unsafe { std::slice::from_raw_parts(authdata, len) })
    }

    /// Get the current x5c value to generate an attestation object.
    /// Usable after creating webauthn credentials.
    pub fn x5c(&self) -> Result<&[u8], Error> {
        let x5c = (self.lib.fido_cred_x5c_ptr)(self.cred, 0);
        if x5c.is_null() {
            bail!("failed to get credentials x5c data pointer");
        }
        let len = (self.lib.fido_cred_x5c_len)(self.cred, 0);
        Ok(unsafe { std::slice::from_raw_parts(x5c, len) })
    }
}

pub struct FidoAssert {
    lib: Arc<Lib>,
    assert: *mut libc::c_void,
}

unsafe impl Send for FidoAssert {}
unsafe impl Sync for FidoAssert {}

impl Drop for FidoAssert {
    fn drop(&mut self) {
        (self.lib.fido_assert_free)(&mut self.assert);
    }
}

impl FidoAssert {
    /// Set FIDO_EXT_HMAC_SECRET.
    pub fn set_hmac_extension(self) -> Result<Self, Error> {
        if (self.lib.fido_assert_set_extensions)(self.assert, 0x01) != 0 {
            bail!("failed to enable hmac extension");
        }
        Ok(self)
    }

    /// Set HMAC salt.
    pub fn set_hmac_salt(self, salt: &[u8]) -> Result<Self, Error> {
        if (self.lib.fido_assert_set_hmac_salt)(self.assert, salt.as_ptr(), salt.len()) != 0 {
            bail!("failed to set hmac salt");
        }
        Ok(self)
    }

    /// Set the relying party id.
    pub fn set_relying_party(self, rpid: &str) -> Result<Self, Error> {
        let rpid = CString::new(rpid).map_err(|_| format_err!("invalid bytes in relying party"))?;
        if (self.lib.fido_assert_set_rp)(self.assert, rpid.as_ptr()) != 0 {
            bail!("failed to set relying party id");
        }
        Ok(self)
    }

    /// Clear the client data hash.
    pub fn clear_clientdata_hash(self) -> Result<Self, Error> {
        let hash = [0u8; 32];
        if (self.lib.fido_assert_set_clientdata_hash)(self.assert, hash.as_ptr(), hash.len()) != 0 {
            bail!("failed to clear clientdata hash");
        }
        Ok(self)
    }

    /// Set the client data hash.
    pub fn set_clientdata_hash(self, hash: &[u8; 32]) -> Result<Self, Error> {
        if (self.lib.fido_assert_set_clientdata_hash)(self.assert, hash.as_ptr(), hash.len()) != 0 {
            bail!("failed to set clientdata hash");
        }
        Ok(self)
    }

    /// Allow a specific client id.
    pub fn allow_cred(self, cid: &[u8]) -> Result<Self, Error> {
        if (self.lib.fido_assert_allow_cred)(self.assert, cid.as_ptr(), cid.len()) != 0 {
            bail!("failed to declare allowed client id");
        }
        Ok(self)
    }

    /// Set user presence requirement.
    pub fn set_user_presence_required(self, on: bool) -> Result<Self, Error> {
        if (self.lib.fido_assert_set_up)(
            self.assert,
            if on { FidoOpt::Omit } else { FidoOpt::False },
        ) != 0
        {
            bail!("failed to set user presence requirement");
        }
        Ok(self)
    }

    /// Set user verification requirement.
    pub fn set_user_verification_required(self, opt: FidoOpt) -> Result<Self, Error> {
        if (self.lib.fido_assert_set_uv)(self.assert, opt) != 0 {
            bail!("failed to set user verification requirement");
        }
        Ok(self)
    }

    /// Get the current hmac secret.
    /// Usable after creating an assertion or making credentials for a HMAC secret.
    pub fn hmac_secret(&self) -> Result<&[u8], Error> {
        let hmac = (self.lib.fido_assert_hmac_secret_ptr)(self.assert, 0);
        if hmac.is_null() {
            bail!("failed to get assertion hmac secret pointer");
        }
        let len = (self.lib.fido_assert_hmac_secret_len)(self.assert, 0);
        Ok(unsafe { std::slice::from_raw_parts(hmac, len) })
    }

    /// Get the current identity.
    /// Usable after creating webauthn assertion.
    pub fn id(&self) -> Result<&[u8], Error> {
        let id = (self.lib.fido_assert_id_ptr)(self.assert, 0);
        if id.is_null() {
            bail!("failed to get assertion id pointer");
        }
        let len = (self.lib.fido_assert_id_len)(self.assert, 0);
        Ok(unsafe { std::slice::from_raw_parts(id, len) })
    }

    /// Get the current signature.
    /// Usable after creating webauthn assertion.
    pub fn signature(&self) -> Result<&[u8], Error> {
        let sig = (self.lib.fido_assert_sig_ptr)(self.assert, 0);
        if sig.is_null() {
            bail!("failed to get assertion signature pointer");
        }
        let len = (self.lib.fido_assert_sig_len)(self.assert, 0);
        Ok(unsafe { std::slice::from_raw_parts(sig, len) })
    }

    /// Get the current auth data.
    /// Usable after creating webauthn assertion.
    pub fn auth_data(&self) -> Result<&[u8], Error> {
        let authdata = (self.lib.fido_assert_authdata_ptr)(self.assert, 0);
        if authdata.is_null() {
            bail!("failed to get assertion auth data pointer");
        }
        let len = (self.lib.fido_assert_authdata_len)(self.assert, 0);
        Ok(unsafe { std::slice::from_raw_parts(authdata, len) })
    }
}
