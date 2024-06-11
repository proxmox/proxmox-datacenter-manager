//! Time formatting functions.

use std::ffi::CStr;
use std::io;

use anyhow::{bail, format_err, Error};

pub fn format_epoch(epoch: i64) -> Result<String, Error> {
    let mut ts: libc::tm = unsafe { std::mem::zeroed() };
    let mut buf = [0u8; 64];
    let epoch = epoch as libc::time_t;
    let len = unsafe {
        if libc::localtime_r(&epoch, &mut ts).is_null() {
            return Err(io::Error::last_os_error().into());
        }

        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            c"%c".as_ptr(),
            &ts,
        )
    };

    if len == 0 || len > buf.len() {
        bail!("failed to format time");
    }

    let text = CStr::from_bytes_with_nul(&buf[..(len + 1)])
        .map_err(|_| format_err!("formatted time string is not valid"))?;

    Ok(text.to_string_lossy().to_string())
}

pub fn format_epoch_lossy(epoch: i64) -> String {
    format_epoch(epoch).unwrap_or_else(|_| format!("{epoch}"))
}
