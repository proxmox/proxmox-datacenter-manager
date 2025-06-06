use std::mem::{ManuallyDrop, MaybeUninit};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{bail, Error};
use nix::sys::stat::Mode;
use once_cell::sync::OnceCell;

use proxmox_sys::fs::{create_path, CreateOptions};

// openssl::sha::sha256(b"Proxmox Backup ConfigVersionCache v1.0")[0..8];
pub const PROXMOX_BACKUP_CONFIG_VERSION_CACHE_MAGIC_1_0: [u8; 8] =
    [25, 198, 168, 230, 154, 132, 143, 131];

const FILE_PATH: &str = pdm_buildcfg::rundir!("/shmem/config-versions");

use proxmox_shared_memory::*;

#[derive(Debug)]
#[repr(C)]
struct ConfigVersionCacheDataInner {
    magic: [u8; 8],
    // User (user.cfg) cache generation/version.
    user_cache_generation: AtomicUsize,
    // Traffic control (traffic-control.cfg) generation/version.
    traffic_control_generation: AtomicUsize,
    // Tracks updates to the remote/hostname/nodename mapping cache.
    remote_mapping_cache: AtomicUsize,
    // Add further atomics here
}

#[repr(C)]
union ConfigVersionCacheData {
    data: ManuallyDrop<ConfigVersionCacheDataInner>,
    _padding: [u8; 4096],
}

#[test]
fn assert_cache_size() {
    assert_eq!(std::mem::size_of::<ConfigVersionCacheData>(), 4096);
}

impl std::ops::Deref for ConfigVersionCacheData {
    type Target = ConfigVersionCacheDataInner;

    #[inline]
    fn deref(&self) -> &ConfigVersionCacheDataInner {
        unsafe { &self.data }
    }
}

impl std::ops::DerefMut for ConfigVersionCacheData {
    #[inline]
    fn deref_mut(&mut self) -> &mut ConfigVersionCacheDataInner {
        unsafe { &mut self.data }
    }
}

impl Init for ConfigVersionCacheData {
    fn initialize(this: &mut MaybeUninit<Self>) {
        unsafe {
            let me = &mut *this.as_mut_ptr();
            me.magic = PROXMOX_BACKUP_CONFIG_VERSION_CACHE_MAGIC_1_0;
        }
    }

    fn check_type_magic(this: &MaybeUninit<Self>) -> Result<(), Error> {
        unsafe {
            let me = &*this.as_ptr();
            if me.magic != PROXMOX_BACKUP_CONFIG_VERSION_CACHE_MAGIC_1_0 {
                bail!("ConfigVersionCache: wrong magic number");
            }
            Ok(())
        }
    }
}

pub struct ConfigVersionCache {
    shmem: SharedMemory<ConfigVersionCacheData>,
}

static INSTANCE: OnceCell<Arc<ConfigVersionCache>> = OnceCell::new();

impl ConfigVersionCache {
    /// Open the memory backed version cache.
    pub fn new() -> Result<Arc<Self>, Error> {
        INSTANCE.get_or_try_init(Self::open).cloned()
    }

    /// Convenience method to call [`ConfigVersionCache::new`] while turning the error into a log
    /// message.
    pub fn new_log_error() -> Option<Arc<Self>> {
        match Self::new() {
            Ok(this) => Some(this),
            Err(err) => {
                log::error!("failed to open config version cache - {err:?}");
                None
            }
        }
    }

    // Actual work of `new`:
    fn open() -> Result<Arc<Self>, Error> {
        let user = crate::api_user()?;

        let dir_opts = CreateOptions::new()
            .perm(Mode::from_bits_truncate(0o770))
            .owner(user.uid)
            .group(user.gid);

        let file_path = Path::new(FILE_PATH);
        let dir_path = file_path.parent().unwrap();

        create_path(dir_path, Some(dir_opts), Some(dir_opts))?;

        let file_opts = CreateOptions::new()
            .perm(Mode::from_bits_truncate(0o660))
            .owner(user.uid)
            .group(user.gid);

        let shmem: SharedMemory<ConfigVersionCacheData> = SharedMemory::open(file_path, file_opts)?;

        Ok(Arc::new(Self { shmem }))
    }

    /// Returns the user cache generation number.
    pub fn user_cache_generation(&self) -> usize {
        self.shmem
            .data()
            .user_cache_generation
            .load(Ordering::Acquire)
    }

    /// Increase the user cache generation number.
    pub fn increase_user_cache_generation(&self) {
        self.shmem
            .data()
            .user_cache_generation
            .fetch_add(1, Ordering::AcqRel);
    }

    /// Returns the traffic control generation number.
    pub fn traffic_control_generation(&self) -> usize {
        self.shmem
            .data()
            .traffic_control_generation
            .load(Ordering::Acquire)
    }

    /// Increase the traffic control generation number.
    pub fn increase_traffic_control_generation(&self) {
        self.shmem
            .data()
            .traffic_control_generation
            .fetch_add(1, Ordering::AcqRel);
    }

    /// Return the current remote mapping cache generation.
    pub fn remote_mapping_cache(&self) -> usize {
        self.shmem
            .data()
            .remote_mapping_cache
            .load(Ordering::Relaxed)
    }

    /// Increase the remote mapping cache number.
    pub fn increase_remote_mapping_cache(&self) -> usize {
        self.shmem
            .data()
            .remote_mapping_cache
            .fetch_add(1, Ordering::Relaxed)
            + 1
    }
}
