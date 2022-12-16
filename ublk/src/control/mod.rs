// SPDX-License-Identifier: MIT

mod sys;

use crate::error::Result;
use bitflags::bitflags;
use io_uring::{cqueue, squeue, IoUring};
use std::fs::OpenOptions;
use std::mem;
use std::os::unix::io::{AsRawFd, OwnedFd};

/// Control object
pub struct UblkCtrl {
    ring: IoUring<squeue::Entry128, cqueue::Entry32>,
    uniq: u64,
    _ctrl_dev: OwnedFd,
}

impl UblkCtrl {
    /// ublk control device path
    pub const CTRL_DEV_PATH: &'static str = "/dev/ublk-control";

    /// ubcltrl constructor
    /// # Errors
    ///
    pub fn new() -> Result<Self> {
        let ring = IoUring::generic_builder().build(32)?;

        let ctrl_dev = OpenOptions::new()
            .read(true)
            .write(true)
            .open(Self::CTRL_DEV_PATH)?;

        ring.submitter().register_files(&[ctrl_dev.as_raw_fd()])?;

        let ctrl = Self {
            ring,
            uniq: 0,
            _ctrl_dev: ctrl_dev.into(),
        };
        Ok(ctrl)
    }

    /// Add new device
    /// # Errors
    ///
    pub fn add_device(&mut self, options: &DeviceOptions) -> Result<DeviceInfo> {
        self.uniq += 1;

        // if after cast `dev_id` < 0, it means requesting a new id
        let mut info = sys::DevInfo::new()
            .dev_id(options.dev_id)
            .max_io_buf_bytes(options.max_io_buf_bytes)
            .nr_hw_queues(options.nr_hw_queues)
            .queue_depth(options.queue_depth)
            .flags(options.flags.bits());

        // The kernel driver fails if info.dev_id != cmd.dev_id
        sys::CtrlCmd::new(sys::CtrlOp::AddDev, options.dev_id)
            .buffer(&mut info)
            .submit_and_wait(self.uniq, &mut self.ring)?;

        Ok(info.into())
    }

    /// Delete a device
    /// # Errors
    ///
    pub fn delete_device(&mut self, dev_id: u32) -> Result<()> {
        self.uniq += 1;

        sys::CtrlCmd::new(sys::CtrlOp::DelDev, dev_id)
            .submit_and_wait(self.uniq, &mut self.ring)?;

        Ok(())
    }

    /// Start the ublksrv device:
    ///
    /// 1) fork a daemon for handling IO command from driver
    ///
    /// 2) wait for the device becoming ready: the daemon should submit
    /// sqes to /dev/ublkcN, just like usb's urb usage, each request needs
    /// one sqe. If one IO request comes to kernel driver of /dev/ublkbN,
    /// the sqe for this request is completed, and the daemon gets notified.
    /// When every io request of driver gets its own sqe queued, we think
    /// /dev/ublkbN is ready to start
    ///
    /// 3) in current process context, sent `StartDev` command to
    /// /dev/ublk-control with device id, which will cause ublk driver to
    /// expose /dev/ublkbN
    /// # Errors
    ///
    pub fn start_device(&mut self, dev_id: u32, pid: u64) -> Result<()> {
        self.uniq += 1;

        sys::CtrlCmd::new(sys::CtrlOp::StartDev, dev_id)
            .data(pid)
            .submit_and_wait(self.uniq, &mut self.ring)?;

        Ok(())
    }

    ///  Stop the ublksrv device:
    ///
    ///  1) send `StopDev` command to /dev/ublk-control with device id provided
    ///
    ///  2) ublk driver gets this command, freeze /dev/ublkbN, then complete all
    ///  pending seq, meantime tell the daemon via cqe->res to not submit sqe
    ///  any more, since we are being closed. Also delete /dev/ublkbN.
    ///
    ///  3) the ublk daemon figures out that all sqes are completed, and free,
    ///  then close /dev/ublkcN and exit itself.
    /// # Errors
    ///
    pub fn stop_device(&mut self, dev_id: u32) -> Result<()> {
        self.uniq += 1;

        sys::CtrlCmd::new(sys::CtrlOp::StopDev, dev_id)
            .submit_and_wait(self.uniq, &mut self.ring)?;

        Ok(())
    }

    /// Set the device parameters
    /// Parameters can only be changed when device isn't live
    /// # Errors
    ///
    pub fn set_device_parameters(&mut self, dev_id: u32, params: &DeviceParams) -> Result<()> {
        self.uniq += 1;

        let mut params: sys::DevParams = params.into();

        sys::CtrlCmd::new(sys::CtrlOp::SetParams, dev_id)
            .buffer(&mut params)
            .submit_and_wait(self.uniq, &mut self.ring)?;

        Ok(())
    }

    /// Get the device parameters
    /// # Errors
    ///
    pub fn get_device_parameters(&mut self, dev_id: u32) -> Result<DeviceParams> {
        self.uniq += 1;

        let mut params = sys::DevParams::empty();

        sys::CtrlCmd::new(sys::CtrlOp::GetParams, dev_id)
            .buffer(&mut params)
            .submit_and_wait(self.uniq, &mut self.ring)?;

        Ok(params.into())
    }

    /// Get device's queue affinity
    ///
    /// This is only used for setting up queue pthread daemons
    /// # Errors
    ///
    pub fn get_queue_affinity(&mut self, dev_id: u32, queue: u16) -> Result<libc::cpu_set_t> {
        self.uniq += 1;

        // SAFETY: all-zero byte-pattern represents a valid libc::cpu_set_t
        let mut cpu_set: libc::cpu_set_t = unsafe { mem::zeroed() };

        sys::CtrlCmd::new(sys::CtrlOp::GetQueueAffinity, dev_id)
            .buffer(&mut cpu_set)
            .data(u64::from(queue))
            .submit_and_wait(self.uniq, &mut self.ring)?;

        Ok(cpu_set)
    }

    /// Get device's queues affinity
    ///
    /// This is only used for setting up queue pthread daemons
    /// # Errors
    ///
    pub fn get_all_queues_affinity(
        &mut self,
        dev_id: u32,
        nr_queues: u16,
    ) -> Result<Vec<libc::cpu_set_t>> {
        let mut set: Vec<libc::cpu_set_t> = Vec::with_capacity(nr_queues as usize);

        for queue in 0..nr_queues {
            let cpu_set = self.get_queue_affinity(dev_id, queue)?;
            set.push(cpu_set);
        }

        Ok(set)
    }

    /// Get the device information
    /// # Errors
    ///
    pub fn get_device_info(&mut self, dev_id: u32) -> Result<DeviceInfo> {
        self.uniq += 1;

        let mut info = sys::DevInfo::new();

        sys::CtrlCmd::new(sys::CtrlOp::GetDevInfo, dev_id)
            .buffer(&mut info)
            .submit_and_wait(self.uniq, &mut self.ring)?;

        Ok(info.into())
    }
}

/// Device information
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Device id
    pub dev_id: u32,
    /// User space server PID
    pub srv_pid: i32,
    /// Device state
    pub active: bool,
    /// Number of hardware queues
    pub nr_hw_queues: u16,
    /// Queue depth
    pub queue_depth: u16,
    /// Request queue size in bytes
    pub max_io_buf_bytes: u32,
    /// Device flags
    pub flags: DeviceFlags,
}

bitflags! {
    /// 64bit flags that will be copied back to userspace as feature negotiation result
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct DeviceFlags: u64 {
        /// zero copy requires 4k block size, and can remap ublk driver's io
        /// request into ublksrv's vm space.
        /// Kernel driver is not ready to support zero copy.
        const ZeroCopy = sys::DevInfo::SUPPORT_ZERO_COPY;

        /// Force to complete io cmd via io_uring_cmd_complete_in_task so that
        /// performance comparison is done easily with using task_work_add
        const ForceIouCmdCompleteInTask = sys::DevInfo::URING_CMD_COMP_IN_TASK;

        /// User should issue io cmd again for write requests to set io buffer address
        /// and copy data from bio vectors to the userspace io buffer.
        /// In this mode, task_work is not used.
        const NeedGetData = sys::DevInfo::NEED_GET_DATA;
    }
}

/// Options and flags which can be used to configure how a ublk device is created.
///
/// This builder exposes the ability to configure how a device is created and
/// the feature negotiation.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DeviceOptions {
    dev_id: u32,
    nr_hw_queues: u16,
    queue_depth: u16,
    max_io_buf_bytes: u32,
    flags: DeviceFlags,
}

impl DeviceOptions {
    /// Maximum device request queue size in bytes
    pub const MAX_BUF_SIZE: u32 = sys::DevInfo::MAX_BUF_SIZE;
    /// Maximum device number of hardware queues
    pub const MAX_NR_HW_QUEUES: u16 = sys::DevInfo::MAX_NR_HW_QUEUES;
    /// Maximum device queue depth
    pub const MAX_QUEUE_DEPTH: u16 = sys::DevInfo::MAX_QUEUE_DEPTH;

    /// Default device request queue size in bytes
    pub const DEFAULT_BUF_SIZE: u32 = sys::DevInfo::DEFAULT_BUF_SIZE;
    /// Default device number of hardware queues
    pub const DEFAULT_NR_HW_QUEUES: u16 = sys::DevInfo::DEFAULT_NR_HW_QUEUES;
    /// Default device queue depth
    pub const DEFAULT_QUEUE_DEPTH: u16 = sys::DevInfo::DEFAULT_QUEUE_DEPTH;

    /// Device options constructor
    #[must_use]
    pub const fn new() -> Self {
        Self {
            dev_id: sys::DevInfo::NEW_DEV_ID,
            nr_hw_queues: Self::DEFAULT_NR_HW_QUEUES,
            queue_depth: Self::DEFAULT_QUEUE_DEPTH,
            max_io_buf_bytes: Self::DEFAULT_BUF_SIZE,
            flags: DeviceFlags::empty(),
        }
    }

    /// Sets the requested device ID
    #[must_use]
    pub const fn device_id(mut self, dev_id: u32) -> Self {
        self.dev_id = dev_id;
        self
    }

    /// Sets the device's number of hardware queues
    #[must_use]
    pub const fn nr_hw_queues(mut self, nr_hw_queues: u16) -> Self {
        self.nr_hw_queues = if nr_hw_queues <= Self::MAX_NR_HW_QUEUES {
            nr_hw_queues
        } else {
            Self::MAX_NR_HW_QUEUES
        };
        self
    }

    /// Sets the device's queue depth
    #[must_use]
    pub const fn queue_depth(mut self, queue_depth: u16) -> Self {
        self.queue_depth = if queue_depth <= Self::MAX_QUEUE_DEPTH {
            queue_depth
        } else {
            Self::MAX_QUEUE_DEPTH
        };
        self
    }

    /// Sets the device's request queue size in bytes
    #[must_use]
    pub const fn max_io_buf_bytes(mut self, max_io_buf_bytes: u32) -> Self {
        self.max_io_buf_bytes = if max_io_buf_bytes <= Self::MAX_BUF_SIZE {
            max_io_buf_bytes
        } else {
            Self::MAX_BUF_SIZE
        };
        self
    }

    /// Sets the device's [`DeviceFlags`] flags
    #[must_use]
    pub const fn flags(mut self, flags: DeviceFlags) -> Self {
        self.flags = flags;
        self
    }
}

impl Default for DeviceOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Device parameters
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct DeviceParams {
    /// Device attributes
    pub attrs: DeviceAttr,
    /// TODO
    pub logical_bs_shift: u8,
    /// TODO
    pub physical_bs_shift: u8,
    /// TODO
    pub io_opt_shift: u8,
    /// TODO
    pub io_min_shift: u8,
    /// TODO
    pub max_sectors: u32,
    /// TODO
    pub chunk_sectors: u32,
    /// TODO
    pub dev_sectors: u64,
    /// TODO
    pub virt_boundary_mask: u64,
    /// Device optional discard parameters
    pub discard: Option<DeviceParamDiscard>,
}

/// Device optional discard parameters
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct DeviceParamDiscard {
    /// TODO
    pub discard_alignment: u32,
    /// TODO
    pub discard_granularity: u32,
    /// TODO
    pub max_discard_sectors: u32,
    /// TODO
    pub max_write_zeroes_sectors: u32,
    /// TODO
    pub max_discard_segments: u16,
}

bitflags! {
    /// Device Attributes flags
    #[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
    pub struct DeviceAttr: u32 {
        /// Read-only device
        const ReadOnly = sys::DevParamBasic::ATTR_READ_ONLY;
        /// Rotational device
        const Rotational = sys::DevParamBasic::ATTR_ROTATIONAL;
        /// A device qith volatile cache
        const VolatileCache = sys::DevParamBasic::ATTR_VOLATILE_CACHE;
        /// FUA support
        const Fua = sys::DevParamBasic::ATTR_FUA;
    }
}
