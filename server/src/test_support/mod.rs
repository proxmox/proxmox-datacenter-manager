#[cfg(remote_config = "faked")]
pub mod fake_remote;

#[cfg(test)]
pub fn init_product_config() {
    static INIT: std::sync::Once = std::sync::Once::new();

    INIT.call_once(|| {
        let user = nix::unistd::User::from_uid(nix::unistd::Uid::current())
            .expect("test user lookup succeeded")
            .expect("test user exists in the user database");
        proxmox_product_config::init(user.clone(), user);
    });
}
