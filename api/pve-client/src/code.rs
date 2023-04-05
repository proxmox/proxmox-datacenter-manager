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

    /// Start a qemu VM.
    pub async fn start_qemu_on_node(
        &self,
        node: &str,
        vmid: u64,
        start_qemu: StartQemu,
    ) -> Result<(), E::Error> {
        let upid = self.start_qemu_async(node, vmid, start_qemu).await?;
        self.poll_upid(upid).await?.map_err(E::Error::task_failed)
    }

    /// Stop a qemu VM.
    pub async fn stop_qemu_on_node(
        &self,
        node: &str,
        vmid: u64,
        stop_qemu: StopQemu,
    ) -> Result<(), E::Error> {
        let upid = self.stop_qemu_async(node, vmid, stop_qemu).await?;
        self.poll_upid(upid).await?.map_err(E::Error::task_failed)
    }

    /// Perform a controlled shutdown of a qemu VM.
    pub async fn shutdown_qemu_on_node(
        &self,
        node: &str,
        vmid: u64,
        shutdown_qemu: ShutdownQemu,
    ) -> Result<(), E::Error> {
        let upid = self.shutdown_qemu_async(node, vmid, shutdown_qemu).await?;
        self.poll_upid(upid).await?.map_err(E::Error::task_failed)
    }

    /// Start a lxc container.
    pub async fn start_lxc_on_node(
        &self,
        node: &str,
        vmid: u64,
        start_lxc: StartLxc,
    ) -> Result<(), E::Error> {
        let upid = self.start_lxc_async(node, vmid, start_lxc).await?;
        self.poll_upid(upid).await?.map_err(E::Error::task_failed)
    }

    /// Stop a lxc container.
    pub async fn stop_lxc_on_node(
        &self,
        node: &str,
        vmid: u64,
        stop_lxc: StopLxc,
    ) -> Result<(), E::Error> {
        let upid = self.stop_lxc_async(node, vmid, stop_lxc).await?;
        self.poll_upid(upid).await?.map_err(E::Error::task_failed)
    }

    /// Perform a controlled shutdown of a lxc container.
    pub async fn shutdown_lxc_on_node(
        &self,
        node: &str,
        vmid: u64,
        shutdown_lxc: ShutdownLxc,
    ) -> Result<(), E::Error> {
        let upid = self.shutdown_lxc_async(node, vmid, shutdown_lxc).await?;
        self.poll_upid(upid).await?.map_err(E::Error::task_failed)
    }
}
