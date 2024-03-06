use proxmox_router::cli::{run_cli_command, CliCommandMap, CliEnvironment};

mod remotes;

fn main() {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment
    proxmox_router::cli::init_cli_logger("PDM_LOG", "info");

    let cmd_def = CliCommandMap::new().insert("remote", remotes::cli());

    let rpcenv = CliEnvironment::new();
    run_cli_command(
        cmd_def,
        rpcenv,
        Some(|future| proxmox_async::runtime::main(future)),
    );
}
