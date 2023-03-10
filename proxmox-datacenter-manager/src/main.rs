use proxmox_router::cli::{run_cli_command, CliCommand, CliCommandMap, CliEnvironment};
use proxmox_schema::api;

fn main() {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment
    proxmox_router::cli::init_cli_logger("PDM_LOG", "info");

    let cmd_def = CliCommandMap::new().insert("hello", CliCommand::new(&API_METHOD_HELLO));

    let rpcenv = CliEnvironment::new();
    run_cli_command(
        cmd_def,
        rpcenv,
        Some(|future| proxmox_async::runtime::main(future)),
    );
}

#[api]
/// Hello command.
fn hello() {
    println!("Hello");
}
