use anyhow::{bail, format_err, Error};
use openssl::pkey::PKey;
use openssl::x509::X509;

use proxmox_router::list_subdirs_api_method;
use proxmox_router::SubdirMap;
use proxmox_router::{Permission, Router, RpcEnvironment};
use proxmox_schema::api;
use proxmox_sys::task_log;

use proxmox_acme_api::{AcmeDomain, CertificateInfo};

use proxmox_rest_server::WorkerTask;
use proxmox_schema::api_types::NODE_SCHEMA;

use pdm_api_types::PRIV_SYS_MODIFY;

use crate::auth::certs::{API_CERT_FN, API_KEY_FN};

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

const SUBDIRS: SubdirMap = &[
    ("acme", &ACME_ROUTER),
    (
        "custom",
        &Router::new()
            .post(&API_METHOD_UPLOAD_CUSTOM_CERTIFICATE)
            .delete(&API_METHOD_DELETE_CUSTOM_CERTIFICATE),
    ),
    ("info", &Router::new().get(&API_METHOD_GET_INFO)),
];

const ACME_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(ACME_SUBDIRS))
    .subdirs(ACME_SUBDIRS);

const ACME_SUBDIRS: SubdirMap = &[(
    "certificate",
    &Router::new()
        .post(&API_METHOD_NEW_ACME_CERT)
        .put(&API_METHOD_RENEW_ACME_CERT),
)];

fn get_certificate_pem() -> Result<Vec<u8>, Error> {
    let cert_pem = proxmox_sys::fs::file_get_contents(API_CERT_FN)?;
    Ok(cert_pem)
}

fn get_certificate_info() -> Result<CertificateInfo, Error> {
    let cert_pem = get_certificate_pem()?;
    CertificateInfo::from_pem("proxy.pem", &cert_pem)
}

#[api(
    input: {
        properties: {
            node: { schema: NODE_SCHEMA },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    returns: {
        type: Array,
        items: { type: CertificateInfo },
        description: "List of certificate infos.",
    },
)]
/// Get certificate info.
pub fn get_info() -> Result<Vec<CertificateInfo>, Error> {
    let info = get_certificate_info()?;

    Ok(vec![info])
}

#[api(
    input: {
        properties: {
            node: { schema: NODE_SCHEMA },
            certificates: { description: "PEM encoded certificate (chain)." },
            key: {
                description: "PEM encoded private key.",
                optional: true,
            },
            // FIXME: widget-toolkit should have an option to disable using these 2 parameters...
            restart: {
                description: "UI compatibility parameter, ignored",
                type: Boolean,
                optional: true,
                default: false,
            },
            force: {
                description: "Force replacement of existing files.",
                type: Boolean,
                optional: true,
                default: false,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    returns: {
        type: Array,
        items: { type: CertificateInfo },
        description: "List of certificate infos.",
    },
    protected: true,
)]
/// Upload a custom certificate.
pub async fn upload_custom_certificate(
    certificates: String,
    key: Option<String>,
) -> Result<Vec<CertificateInfo>, Error> {
    let certificates = X509::stack_from_pem(certificates.as_bytes())
        .map_err(|err| format_err!("failed to decode certificate chain: {}", err))?;

    let key = match key {
        Some(key) => key,
        None => proxmox_sys::fs::file_read_string(API_KEY_FN)?,
    };

    let key = PKey::private_key_from_pem(key.as_bytes())
        .map_err(|err| format_err!("failed to parse private key: {}", err))?;

    let certificates = certificates
        .into_iter()
        .try_fold(Vec::<u8>::new(), |mut stack, cert| -> Result<_, Error> {
            if !stack.is_empty() {
                stack.push(b'\n');
            }
            stack.extend(cert.to_pem()?);
            Ok(stack)
        })
        .map_err(|err| format_err!("error formatting certificate chain as PEM: {}", err))?;

    let key = key.private_key_to_pem_pkcs8()?;

    crate::auth::certs::set_api_certificate(&certificates, &key)?;

    crate::reload_api_certificate().await?;

    get_info()
}

#[api(
    input: {
        properties: {
            node: { schema: NODE_SCHEMA },
            restart: {
                description: "UI compatibility parameter, ignored",
                type: Boolean,
                optional: true,
                default: false,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Delete the current certificate and regenerate a self signed one.
pub async fn delete_custom_certificate() -> Result<(), Error> {
    // Here we fail since if this fails nothing else breaks anyway
    std::fs::remove_file(API_CERT_FN)
        .map_err(|err| format_err!("failed to unlink {:?} - {}", API_CERT_FN, err))?;

    if let Err(err) = std::fs::remove_file(API_KEY_FN) {
        // Here we just log since the certificate is already gone and we'd rather try to generate
        // the self-signed certificate even if this fails:
        log::error!(
            "failed to remove certificate private key {:?} - {}",
            API_KEY_FN,
            err
        );
    }

    crate::auth::certs::update_self_signed_cert(true)?;

    crate::reload_api_certificate().await?;

    Ok(())
}

#[api(
    input: {
        properties: {
            node: { schema: NODE_SCHEMA },
            force: {
                description: "Force replacement of existing files.",
                type: Boolean,
                optional: true,
                default: false,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Order a new ACME certificate.
pub fn new_acme_cert(force: bool, rpcenv: &mut dyn RpcEnvironment) -> Result<String, Error> {
    spawn_certificate_worker("acme-new-cert", force, rpcenv)
}

#[api(
    input: {
        properties: {
            node: { schema: NODE_SCHEMA },
            force: {
                description: "Force replacement of existing files.",
                type: Boolean,
                optional: true,
                default: false,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Renew the current ACME certificate if it expires within 30 days (or always if the `force`
/// parameter is set).
pub fn renew_acme_cert(force: bool, rpcenv: &mut dyn RpcEnvironment) -> Result<String, Error> {
    if !cert_expires_soon()? && !force {
        bail!("Certificate does not expire within the next 30 days and 'force' is not set.")
    }

    spawn_certificate_worker("acme-renew-cert", force, rpcenv)
}

/// Check whether the current certificate expires within the next 30 days.
pub fn cert_expires_soon() -> Result<bool, Error> {
    let cert = get_certificate_info()?;
    cert.is_expired_after_epoch(proxmox_time::epoch_i64() + 30 * 24 * 60 * 60)
        .map_err(|err| format_err!("Failed to check certificate expiration date: {}", err))
}

fn spawn_certificate_worker(
    name: &'static str,
    force: bool,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<String, Error> {
    // We only have 1 certificate path in PBS which makes figuring out whether or not it is a
    // custom one too hard... We keep the parameter because the widget-toolkit may be using it...
    let _ = force;

    let (cert_config, _digest) = pdm_config::certificate_config::config()?;

    let auth_id = rpcenv.get_auth_id().unwrap();

    let domains = cert_config.acme_domains().try_fold(
        Vec::<AcmeDomain>::new(),
        |mut acc, domain| -> Result<_, Error> {
            let mut domain = domain?;
            domain.domain.make_ascii_lowercase();
            if let Some(alias) = &mut domain.alias {
                alias.make_ascii_lowercase();
            }
            acc.push(domain);
            Ok(acc)
        },
    )?;

    let acme_config = if let Some(cfg) = cert_config.acme_config().transpose()? {
        cfg
    } else {
        proxmox_acme_api::parse_acme_config_string("default")?
    };

    WorkerTask::spawn(name, None, auth_id, true, move |worker| async move {
        let work = || async {
            if let Some(cert) =
                proxmox_acme_api::order_certificate(worker, &acme_config, &domains).await?
            {
                crate::auth::certs::set_api_certificate(&cert.certificate, &cert.private_key_pem)?;
                crate::reload_api_certificate().await?;
            }

            Ok(())
        };

        let res = work().await;

        // fixme: send_certificate_renewal_mail(&res)?;

        res
    })
}

#[api(
    input: {
        properties: {
            node: { schema: NODE_SCHEMA },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Renew the current ACME certificate if it expires within 30 days (or always if the `force`
/// parameter is set).
pub fn revoke_acme_cert(rpcenv: &mut dyn RpcEnvironment) -> Result<String, Error> {
    let (cert_config, _digest) = pdm_config::certificate_config::config()?;

    let cert_pem = get_certificate_pem()?;

    let auth_id = rpcenv.get_auth_id().unwrap();

    let acme_config = if let Some(cfg) = cert_config.acme_config().transpose()? {
        cfg
    } else {
        proxmox_acme_api::parse_acme_config_string("default")?
    };

    WorkerTask::spawn(
        "acme-revoke-cert",
        None,
        auth_id,
        true,
        move |worker| async move {
            task_log!(worker, "Revoking old certificate");
            proxmox_acme_api::revoke_certificate(&acme_config, &cert_pem).await?;
            task_log!(
                worker,
                "Deleting certificate and regenerating a self-signed one"
            );
            delete_custom_certificate().await?;
            Ok(())
        },
    )
}
