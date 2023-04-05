use std::time::Duration;

use proxmox_client::{ApiResponse, Environment, Error};

use crate::helpers::*;
use crate::types::*;
use crate::Client;

include!("generated/code.rs");

impl<E> Client<E>
where
    E: Environment,
    E::Error: From<anyhow::Error>,
    anyhow::Error: From<E::Error>,
{
    /// This does not utilize `Error::task_failed` directly and returns an error only if an API
    /// call fails.
    ///
    /// If the task ended successfully, this returns `Ok(Ok(()))`. If the task failed, it returns
    /// `Ok(Err(<exit status>))`.
    pub async fn poll_upid(&self, upid: PveUpid) -> Result<Result<(), String>, E::Error> {
        let status = loop {
            let status = self.get_task_status(&upid.node, &upid.to_string()).await?;
            if !status.is_running() {
                break status;
            }
            E::sleep(Duration::from_secs(1))?.await;
        };

        let msg = status
            .exitstatus
            .ok_or_else(|| E::Error::bad_api("no exit status on finished task"))?;

        Ok(if msg.ends_with("OK") {
            Ok(())
        } else {
            Err(msg)
        })
    }
}
