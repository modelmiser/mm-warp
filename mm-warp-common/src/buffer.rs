use anyhow::{Context, Result};
use std::os::fd::OwnedFd;
use memmap2::MmapMut;
use nix::sys::memfd;
use nix::unistd::ftruncate;

/// Create a memfd-backed mmap suitable for Wayland shared memory.
///
/// Returns the OwnedFd (needed for wl_shm_pool) and the writable MmapMut.
pub fn create_memfd_mmap(name: &str, size: usize) -> Result<(OwnedFd, MmapMut)> {
    // Build null-terminated name for memfd
    let cname = std::ffi::CString::new(name)
        .context("Invalid memfd name")?;

    let fd = memfd::memfd_create(
        &cname,
        memfd::MemFdCreateFlag::MFD_CLOEXEC,
    ).context("Failed to create memfd")?;

    ftruncate(&fd, size as i64).context("Failed to truncate memfd")?;

    let mmap = unsafe {
        MmapMut::map_mut(&fd).context("Failed to mmap")?
    };

    Ok((fd, mmap))
}
