use proxmox_router::RpcEnvironment;

/// An [`RpcEnvironment`] that can be used in integration tests.
pub struct TestRpcEnvironment {
    pub attribs: serde_json::Value,
}

impl RpcEnvironment for TestRpcEnvironment {
    fn result_attrib_mut(&mut self) -> &mut serde_json::Value {
        &mut self.attribs
    }

    fn result_attrib(&self) -> &serde_json::Value {
        &self.attribs
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
