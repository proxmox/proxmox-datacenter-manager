use anyhow::{bail, Error};

use proxmox_schema::{api, const_regex, ApiStringFormat, ApiType, Schema, StringSchema};

include!("generated/types.rs");

/// A PVE Upid, contrary to a PBS Upid, contains no 'task-id' number.
pub struct PveUpid {
    /// The Unix PID
    pub pid: i32, // really libc::pid_t, but we don't want this as a dependency for proxmox-schema
    /// The Unix process start time from `/proc/pid/stat`
    pub pstart: u64,
    /// The task start time (Epoch)
    pub starttime: i64,
    /// Worker type (arbitrary ASCII string)
    pub worker_type: String,
    /// Worker ID (arbitrary ASCII string)
    pub worker_id: Option<String>,
    /// The authenticated entity who started the task
    pub auth_id: String,
    /// The node name.
    pub node: String,
}

const_regex! {
    pub PROXMOX_UPID_REGEX = concat!(
        r"^UPID:(?P<node>[a-zA-Z0-9]([a-zA-Z0-9\-]*[a-zA-Z0-9])?):(?P<pid>[0-9A-Fa-f]{8}):",
        r"(?P<pstart>[0-9A-Fa-f]{8,9}):(?P<starttime>[0-9A-Fa-f]{8}):",
        r"(?P<wtype>[^:\s]+):(?P<wid>[^:\s]*):(?P<authid>[^:\s]+):$"
    );
}

pub const PROXMOX_UPID_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&PROXMOX_UPID_REGEX);

pub const UPID_SCHEMA: Schema = StringSchema::new("Unique Process/Task Identifier")
    .min_length("UPID:N:12345678:12345678:12345678:::".len())
    .format(&PROXMOX_UPID_FORMAT)
    .schema();

impl ApiType for PveUpid {
    const API_SCHEMA: Schema = UPID_SCHEMA;
}

impl std::str::FromStr for PveUpid {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(cap) = PROXMOX_UPID_REGEX.captures(s) {
            let worker_id = if cap["wid"].is_empty() {
                None
            } else {
                let wid = unescape_id(&cap["wid"])?;
                Some(wid)
            };

            Ok(PveUpid {
                pid: i32::from_str_radix(&cap["pid"], 16).unwrap(),
                pstart: u64::from_str_radix(&cap["pstart"], 16).unwrap(),
                starttime: i64::from_str_radix(&cap["starttime"], 16).unwrap(),
                worker_type: cap["wtype"].to_string(),
                worker_id,
                auth_id: cap["authid"].to_string(),
                node: cap["node"].to_string(),
            })
        } else {
            bail!("unable to parse UPID '{}'", s);
        }
    }
}

impl std::fmt::Display for PveUpid {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let wid = if let Some(ref id) = self.worker_id {
            escape_id(id)
        } else {
            String::new()
        };

        // Note: pstart can be > 32bit if uptime > 497 days, so this can result in
        // more that 8 characters for pstart

        write!(
            f,
            "UPID:{}:{:08X}:{:08X}:{:08X}:{}:{}:{}:",
            self.node, self.pid, self.pstart, self.starttime, self.worker_type, wid, self.auth_id
        )
    }
}

impl serde::Serialize for PveUpid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&ToString::to_string(self))
    }
}

impl<'de> serde::Deserialize<'de> for PveUpid {
    fn deserialize<D>(deserializer: D) -> Result<PveUpid, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ForwardToStrVisitor;

        impl<'a> serde::de::Visitor<'a> for ForwardToStrVisitor {
            type Value = PveUpid;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a valid PVE UPID")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<PveUpid, E> {
                v.parse::<PveUpid>().map_err(|_| {
                    serde::de::Error::invalid_value(serde::de::Unexpected::Str(v), &self)
                })
            }
        }

        deserializer.deserialize_str(ForwardToStrVisitor)
    }
}

// FIXME: This is in `proxmox_schema::upid` and should be `pub` there instead.
/// systemd-unit compatible escaping
fn unescape_id(text: &str) -> Result<String, Error> {
    let mut i = text.as_bytes();

    let mut data: Vec<u8> = Vec::new();

    loop {
        if i.is_empty() {
            break;
        }
        let next = i[0];
        if next == b'\\' {
            if i.len() < 4 || i[1] != b'x' {
                bail!("error in escape sequence");
            }
            let h1 = hex_digit(i[2])?;
            let h0 = hex_digit(i[3])?;
            data.push(h1 << 4 | h0);
            i = &i[4..]
        } else if next == b'-' {
            data.push(b'/');
            i = &i[1..]
        } else {
            data.push(next);
            i = &i[1..]
        }
    }

    let text = String::from_utf8(data)?;

    Ok(text)
}

// FIXME: This is in `proxmox_schema::upid` and should be `pub` there instead.
/// non-path systemd-unit compatible escaping
fn escape_id(unit: &str) -> String {
    use std::fmt::Write;

    let mut escaped = String::new();

    for (i, &c) in unit.as_bytes().iter().enumerate() {
        if c == b'/' {
            escaped.push('-');
        } else if (i == 0 && c == b'.')
            || !matches!(c, b'_' | b'.' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z')
        {
            // unwrap: writing to a String
            write!(escaped, "\\x{:02x}", c).unwrap();
        } else {
            escaped.push(char::from(c));
        }
    }

    escaped
}

fn hex_digit(d: u8) -> Result<u8, Error> {
    match d {
        b'0'..=b'9' => Ok(d - b'0'),
        b'A'..=b'F' => Ok(d - b'A' + 10),
        b'a'..=b'f' => Ok(d - b'a' + 10),
        _ => bail!("got invalid hex digit"),
    }
}

impl IsRunning {
    pub fn is_running(self) -> bool {
        self == IsRunning::Running
    }
}

impl TaskStatus {
    pub fn is_running(&self) -> bool {
        self.status.is_running()
    }
}
