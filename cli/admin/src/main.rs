use proxmox_router::cli::{run_cli_command, CliCommandMap, CliEnvironment};

mod remotes;

fn main() {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment
    proxmox_log::init_cli_logger("PDM_LOG", proxmox_log::LevelFilter::INFO)
        .expect("failed to set up logger");

    server::context::init().expect("could not set up server context");

    let cmd_def = CliCommandMap::new().insert("remote", remotes::cli());

    let rpcenv = CliEnvironment::new();
    run_cli_command(
        cmd_def,
        rpcenv,
        Some(|future| proxmox_async::runtime::main(future)),
    );
}
