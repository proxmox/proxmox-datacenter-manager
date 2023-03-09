use anyhow::Error;

use proxmox_sys::fs::{create_path, CreateOptions};

use pdm_buildcfg;

/// Create the base run-directory.
///
/// This exists to fixate the permissions for the run *base* directory while allowing intermediate
/// directories after it to have different permissions.
pub fn create_run_dir() -> Result<(), Error> {
    let user = pdm_config::api_user()?;
    let opts = CreateOptions::new().owner(user.uid).group(user.gid);
    let _: bool = create_path(pdm_buildcfg::PDM_RUN_DIR_M!(), None, Some(opts))?;
    Ok(())
}

pub fn create_state_dir() -> Result<(), Error> {
    let user = pdm_config::api_user()?;
    let opts = CreateOptions::new().owner(user.uid).group(user.gid);
    create_path(
        pdm_buildcfg::PDM_STATE_DIR_M!(),
        None,
        Some(opts),
    )?;
    Ok(())
}

pub fn sanitize_environment_vars() {
    std::env::set_var("PATH", "/sbin:/bin:/usr/sbin:/usr/bin");
    // Make %ENV safer - as suggested by https://perldoc.perl.org/perlsec.html
    for name in &["IFS", "CDPATH", "ENV", "BASH_ENV"] {
        std::env::remove_var(name);
    }
}
