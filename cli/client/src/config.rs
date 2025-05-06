//! Deal with client connections via config, env vars or CLI parameters.

use std::io;
use std::sync::OnceLock;

use anyhow::{bail, format_err, Context as _, Error};
use serde::{Deserialize, Serialize};

use proxmox_auth_api::types::Userid;
use proxmox_router::cli::OutputFormat;
use proxmox_schema::api_types::DNS_NAME_OR_IP_SCHEMA;
use proxmox_schema::{
    api, ApiStringFormat, ApiType, EnumEntry, OneOfSchema, Schema, StringSchema, Updater,
};
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};
use proxmox_section_config::{SectionConfig, SectionConfigPlugin};

use crate::env::Fingerprint;
use crate::XDG;

const CONFIG_FILE_NAME: &str = xdg_path!("config");

#[api(
    properties: {
        color: { optional: true },
        "output-format": { optional: true },
    },
)]
/// Generic global CLI parameters affecting the output formatting.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct FormatArgs {
    /// Use colored output in the terminal.
    #[serde(default)]
    pub color: crate::env::UseColor,

    #[serde(default)]
    pub output_format: OutputFormat,
}

/// If the server includes a userid, return the userid and host parts separately.
fn parse_userid_at_host(host: &str) -> Result<Option<(&str, &str)>, Error> {
    let Some((userid, host)) = host.rsplit_once('@') else {
        return Ok(None);
    };

    if !userid.contains('@') {
        bail!("invalid userid in 'user@host', should be 'user@realm@host'");
    }

    Ok(Some((userid, host)))
}

/// If the server includes a port, return the host and port parts separately.
fn parse_host_port(host: &str) -> Result<Option<(&str, u16)>, Error> {
    let Some((host, port)) = host.rsplit_once(':') else {
        return Ok(None);
    };

    let port: u16 = port
        .parse()
        .map_err(|_| format_err!("invalid port: {port:?}"))?;

    Ok(Some((host, port)))
}

fn optional_env(name: &str) -> Result<Option<String>, Error> {
    match std::env::var(name) {
        Ok(var) => Ok(Some(var)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            bail!("invalid value in {name} variable");
        }
    }
}

#[api(
    properties: {
        host: { optional: true },
        user: { optional: true },
        port: {
            optional: true,
            default: 8443,
        },
        fingerprint: {
            type: String,
            optional: true,
        },
        "password-file": { optional: true },
        "password-command": { optional: true },
    }
)]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Global command line options regarding the PDM instance to connect to.
pub struct PdmConnectArgs {
    /// Server to connect to, or `user@realm@host` triple.
    pub host: Option<String>,

    /// User to use. This overrides the user specified in `--server` if it was included.
    pub user: Option<Userid>,

    /// Port to connect.
    pub port: Option<u16>,

    /// Certificate fingerprint to expect.
    pub fingerprint: Option<Fingerprint>,

    /// File to read the password from.
    password_file: Option<String>,

    /// Command to run to get the password.
    password_command: Option<String>,
}

impl PdmConnectArgs {
    /// Starting from the parsed CLI options, this completes, if possible,  the connection
    /// parameters based on environment and configuration.
    pub fn finalize(&mut self) -> Result<(), Error> {
        self.fill_from_env()?;
        self.fill_from_config()?;
        Ok(())
    }

    /// Normalize the current variables: if the server includes a username, move it to the user
    /// option.
    fn normalize(&mut self) -> Result<(), Error> {
        if let Some(host) = &self.host {
            if let Some((user, host)) = parse_userid_at_host(host)? {
                if self.user.is_none() {
                    self.user = Some(user.parse()?);
                }
                self.host = Some(host.to_string());
            }
        }

        if let Some(host) = &self.host {
            if let Some((host, port)) = parse_host_port(host)? {
                if self.port.is_none() {
                    self.port = Some(port);
                }
                self.host = Some(host.to_string());
            }
        }

        Ok(())
    }

    /// Fill unset parts from the environment.
    fn fill_from_env(&mut self) -> Result<(), Error> {
        self.normalize()?;

        if self.port.is_none() {
            if let Some(port) = optional_env("PDM_PORT")? {
                self.port = Some(
                    port.parse()
                        .map_err(|_| format_err!("invalid port in PDM_PORT variable: {port:?}"))?,
                );
            }
        }

        if self.user.is_none() {
            if let Some(user) = optional_env("PDM_USER")? {
                self.user = Some(user.parse()?);
            }
        }

        if self.host.is_none() {
            self.host = optional_env("PDM_HOST")?;
        }

        if self.fingerprint.is_none() {
            if let Some(fp) = optional_env("PDM_HOST_FINGERPRINT")? {
                self.fingerprint =
                    Some(fp.parse().map_err(|_| {
                        format_err!("PDM_HOST_FINGERPRINT is not a valid fingerprint")
                    })?);
            }
        }

        if self.password_file.is_none() {
            self.password_file = optional_env("PDM_PASSWORD_FILE")?;
        }

        if self.password_command.is_none() {
            self.password_command = optional_env("PDM_PASSWORD_COMMAND")?;
        }

        self.normalize()?;

        Ok(())
    }

    /// Load the current configuration and try to fill in the connection parameters with it.
    fn fill_from_config(&mut self) -> Result<(), Error> {
        // Make sure user and host are separated first.
        self.normalize()?;

        self.fill_from_config_data(&load_config()?)
    }

    /// Load the current configuration and try to fill in the connection parameters with it.
    fn fill_from_config_data(
        &mut self,
        config: &SectionConfigData<ConfigEntry>,
    ) -> Result<(), Error> {
        // Then we use the host name as key in the configuration.
        let Some(host) = self.host.as_deref() else {
            return Ok(());
        };
        if let Some(entry) = config.get(host) {
            let ConfigEntry::Remote(remote) = entry;
            /*
            else {
                log::debug!("host {host:?} exists in the config but is not a remote entry");
                return Ok(());
            };
            */

            if remote.address.is_some() {
                self.host.clone_from(&remote.address);
            }

            if self.port.is_none() {
                self.port = remote.args.port;
            }

            if self.user.is_none() {
                self.user.clone_from(&remote.args.user);
            }

            if self.fingerprint.is_none() {
                self.fingerprint.clone_from(&remote.args.fingerprint);
            }

            if self.password_file.is_none() {
                self.password_file.clone_from(&remote.args.password_file);
            }

            if self.password_command.is_none() {
                self.password_command
                    .clone_from(&remote.args.password_command);
            }
        }

        self.normalize()?;

        Ok(())
    }

    pub fn get_password(&self) -> Result<Option<String>, Error> {
        if let Some(file) = &self.password_file {
            match std::fs::read_to_string(file) {
                Ok(mut pw) => {
                    if pw.ends_with('\n') {
                        let _ = pw.pop();
                    }
                    return Ok(Some(pw));
                }
                Err(err) => {
                    if err.kind() != io::ErrorKind::NotFound {
                        return Err(Error::from(err).context(format!("error reading {file:?}")));
                    }
                }
            }
        }

        if let Some(cmd) = &self.password_command {
            let result = std::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .stderr(std::process::Stdio::inherit())
                .output()?;
            if !result.status.success() {
                bail!("password command exited with errors");
            }
            let mut pw = String::from_utf8(result.stdout)
                .context("password command returned non-utf8 data")?;
            if pw.ends_with('\n') {
                let _ = pw.pop();
            }
            return Ok(Some(pw));
        }

        Ok(None)
    }

    /*
    pub fn user(&self) -> Option<&Userid> {
        self.user.as_ref()
    }

    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn fingerprint(&self) -> Option<&Fingerprint> {
        self.fingerprint.as_ref()
    }
    */
}

#[api(
    properties: {
        args: { flatten: true },
        address: {
            optional: true,
            schema: DNS_NAME_OR_IP_SCHEMA,
        },
    }
)]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Global command line options regarding the PDM instance to connect to.
pub struct RemoteEntry {
    #[serde(flatten)]
    args: PdmConnectArgs,

    /// Server to connect to instead of the hostname this section is named after.
    /// This can be used to create aliases.
    address: Option<String>,
}

/// This is a section in the client config at `XDG_CONFIG_HOME/proxmox-datacenter-manager-client`.
#[derive(Clone, Debug, Deserialize, Serialize, Updater)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ConfigEntry {
    Remote(RemoteEntry),
}

// FIXME: api macro needs a way to generate this from a section-config compatible enum!
impl ApiType for ConfigEntry {
    const API_SCHEMA: Schema = OneOfSchema::new(
        "A clent configuration entry.",
        &(
            "type",
            false,
            &StringSchema::new("The configuration entry type")
                .format(&ApiStringFormat::Enum(&[EnumEntry::new(
                    "remote",
                    "a Proxmox Datacenter Manager remote entry",
                )]))
                .schema(),
        ),
        &[("pve", &RemoteEntry::API_SCHEMA)],
    )
    .schema();
}

impl ApiSectionDataEntry for ConfigEntry {
    const INTERNALLY_TAGGED: Option<&'static str> = Some("type");

    fn section_config() -> &'static SectionConfig {
        static CONFIG: OnceLock<SectionConfig> = OnceLock::new();

        // This is a const instead of inlined below so the unwrap is checked at compile time!
        const REMOTE_ENTRY_SCHEMA: &proxmox_schema::AllOfSchema =
            RemoteEntry::API_SCHEMA.unwrap_all_of_schema();

        CONFIG.get_or_init(|| {
            let mut this = SectionConfig::new(&DNS_NAME_OR_IP_SCHEMA);
            this.register_plugin(SectionConfigPlugin::new(
                "remote".to_string(),
                Some("host".to_string()),
                REMOTE_ENTRY_SCHEMA,
            ));
            this
        })
    }

    fn section_type(&self) -> &'static str {
        match self {
            Self::Remote(_) => "remote",
        }
    }
}

pub fn load_config() -> Result<SectionConfigData<ConfigEntry>, Error> {
    let Some(config_path) = XDG.find_config_file(CONFIG_FILE_NAME) else {
        return Ok(SectionConfigData::default());
    };

    let config_path_str = config_path.as_os_str().to_string_lossy();

    match std::fs::read_to_string(&config_path) {
        Ok(content) => ConfigEntry::parse_section_config(&*config_path_str, &content),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(SectionConfigData::default()),
        Err(err) => {
            Err(Error::from(err).context(format!("failed to load config from {config_path:?}")))
        }
    }
}
