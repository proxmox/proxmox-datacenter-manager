use anyhow::{bail, format_err, Error};
use http::uri::Authority;

use proxmox_client::{Client, TlsOptions};

use pdm_api_types::remotes::{Remote, RemoteType};

/// Connection Info returned from [prepare_connect_client]
struct ConnectInfo {
    pub client: Client,
    pub prefix: String,
    pub perl_compat: bool,
}

/// Returns a [proxmox_client::Client] and a token prefix for the specified [pdm_api_types::Remote]
fn prepare_connect_client(remote: &Remote) -> Result<ConnectInfo, Error> {
    let node = remote
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for remote"))?;
    let mut options = TlsOptions::default();

    if let Some(fp) = &node.fingerprint {
        options = TlsOptions::parse_fingerprint(fp)?;
    }

    let host_port: Authority = node.hostname.parse()?;

    let (default_port, prefix, perl_compat, pve_compat) = match remote.ty {
        RemoteType::Pve => (8006, "PVEAPIToken".to_string(), true, true),
        RemoteType::Pbs => (8007, "PBSAPIToken".to_string(), false, false),
    };

    let uri: http::uri::Uri = format!(
        "https://{}:{}",
        host_port.host(),
        host_port.port_u16().unwrap_or(default_port)
    )
    .parse()?;

    let mut client =
        proxmox_client::Client::with_options(uri.clone(), options, Default::default())?;
    client.set_pve_compatibility(pve_compat);

    Ok(ConnectInfo {
        client,
        prefix,
        perl_compat,
    })
}

pub fn connect(remote: &Remote) -> Result<Client, anyhow::Error> {
    let ConnectInfo {
        client,
        perl_compat,
        prefix,
    } = prepare_connect_client(remote)?;
    client.set_authentication(proxmox_client::Token {
        userid: remote.authid.to_string(),
        prefix,
        value: remote.token.to_string(),
        perl_compat,
    });

    Ok(client)
}

pub async fn connect_or_login(remote: &Remote) -> Result<Client, anyhow::Error> {
    if remote.authid.is_token() {
        connect(remote)
    } else {
        let info = prepare_connect_client(remote)?;
        let client = info.client;
        match client
            .login(proxmox_login::Login::new(
                client.api_url().to_string(),
                remote.authid.to_string(),
                remote.token.to_string(),
            ))
            .await
        {
            Ok(Some(_)) => bail!("two factor auth not supported"),
            Ok(None) => {}
            Err(err) => match err {
                // FIXME: check why Api with 401 is returned instead of an Authentication error
                proxmox_client::Error::Api(code, _) if code.as_u16() == 401 => {
                    bail!("authentication failed")
                }
                proxmox_client::Error::Authentication(_) => {
                    bail!("authentication failed")
                }
                _ => return Err(err.into()),
            },
        }
        Ok(client)
    }
}
