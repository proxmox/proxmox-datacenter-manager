use proxmox_router::RpcEnvironment;

pub struct TestRpcEnvironment;

impl RpcEnvironment for TestRpcEnvironment {
    fn result_attrib_mut(&mut self) -> &mut serde_json::Value {
        unimplemented!()
    }

    fn result_attrib(&self) -> &serde_json::Value {
        unimplemented!()
    }

    fn env_type(&self) -> proxmox_router::RpcEnvironmentType {
        unimplemented!()
    }

    fn set_auth_id(&mut self, _user: Option<String>) {
        unimplemented!()
    }

    fn get_auth_id(&self) -> Option<String> {
        Some("root@pam".to_string())
    }
}
