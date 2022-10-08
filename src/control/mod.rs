// SPDX-License-Identifier: MIT

mod sys;

use crate::control::sys::{CtrlOp, WireCmd, WireDevInfo};
use bitflags::bitflags;
use io_uring::opcode::UringCmd80;
use io_uring::types::Fixed;
use io_uring::{cqueue, squeue, IoUring};
use std::fmt::{Display, Formatter};
use std::fs::OpenOptions;
use std::io;
use std::os::unix::io::{AsRawFd, OwnedFd};

/// Device information
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub dev_id: u32,
    pub srv_pid: i32,
    pub state: DeviceStatus,
    pub nr_hw_queues: u16,
    pub queue_depth: u16,
    pub max_io_buf_bytes: u32,
    pub flags: DeviceFlags,
}

/// Device state
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DeviceStatus {
    /// The device is not active
    Dead = 0,
    /// The device is active
    Live = 1,
}

impl Display for DeviceStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceStatus::Dead => write!(f, "Dead"),
            DeviceStatus::Live => write!(f, "Live"),
        }
    }
}

impl TryFrom<u16> for DeviceStatus {
    type Error = ();

    fn try_from(v: u16) -> Result<Self, Self::Error> {
        match v {
            v if v == Self::Dead as u16 => Ok(Self::Dead),
            v if v == Self::Live as u16 => Ok(Self::Live),
            _ => Err(()),
        }
    }
}

bitflags! {
    /// 64bit flags that will be copied back to userspace as feature negotiation result
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct DeviceFlags: u64 {
        /// zero copy requires 4k block size, and can remap ublk driver's io
        /// request into ublksrv's vm space.
        /// Kernel driver is not ready to support zero copy.
        const ZeroCopy = WireDevInfo::SUPPORT_ZERO_COPY;

        /// Force to complete io cmd via io_uring_cmd_complete_in_task so that
        /// performance comparison is done easily with using task_work_add
        const ForceIouCmdCompleteInTask = WireDevInfo::URING_CMD_COMP_IN_TASK;

        /// User should issue io cmd again for write requests to set io buffer address
        /// and copy data from bio vectors to the userspace io buffer.
        /// In this mode, task_work is not used.
        const NeedGetData = WireDevInfo::NEED_GET_DATA;
    }
}

/// Options and flags which can be used to configure how a ublk device is created.
///
/// This builder exposes the ability to configure how a device is created and
/// the feature negotiation.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DeviceOptions {
    nr_hw_queues: u16,
    queue_depth: u16,
    max_io_buf_bytes: u32,
    flags: DeviceFlags,
}

impl DeviceOptions {
    pub const MAX_BUF_SIZE: u32 = WireDevInfo::MAX_BUF_SIZE;
    pub const MAX_NR_HW_QUEUES: u16 = WireDevInfo::MAX_NR_HW_QUEUES;
    pub const MAX_QUEUE_DEPTH: u16 = WireDevInfo::MAX_QUEUE_DEPTH;

    pub const DEFAULT_BUF_SIZE: u32 = WireDevInfo::DEFAULT_BUF_SIZE;
    pub const DEFAULT_NR_HW_QUEUES: u16 = WireDevInfo::DEFAULT_NR_HW_QUEUES;
    pub const DEFAULT_QUEUE_DEPTH: u16 = WireDevInfo::DEFAULT_QUEUE_DEPTH;

    pub fn new() -> Self {
        Self {
            nr_hw_queues: Self::DEFAULT_NR_HW_QUEUES,
            queue_depth: Self::DEFAULT_QUEUE_DEPTH,
            max_io_buf_bytes: Self::DEFAULT_BUF_SIZE,
            flags: DeviceFlags::empty(),
        }
    }

    pub fn nr_hw_queues(mut self, nr_hw_queues: u16) -> Self {
        self.nr_hw_queues = (nr_hw_queues <= Self::MAX_NR_HW_QUEUES)
            .then(|| nr_hw_queues)
            .unwrap_or(Self::MAX_NR_HW_QUEUES);

        self
    }

    pub fn queue_depth(mut self, queue_depth: u16) -> Self {
        self.queue_depth = (queue_depth <= Self::MAX_QUEUE_DEPTH)
            .then(|| queue_depth)
            .unwrap_or(Self::MAX_QUEUE_DEPTH);

        self
    }

    pub fn max_io_buf_bytes(mut self, max_io_buf_bytes: u32) -> Self {
        self.max_io_buf_bytes = (max_io_buf_bytes <= Self::MAX_BUF_SIZE)
            .then(|| max_io_buf_bytes)
            .unwrap_or(Self::MAX_BUF_SIZE);

        self
    }

    pub fn flags(mut self, flags: DeviceFlags) -> Self {
        self.flags = flags;
        self
    }
}

impl Default for DeviceOptions {
    fn default() -> Self {
        Self::new()
    }
}

pub struct UblkCtrl {
    ring: IoUring<squeue::Entry128, cqueue::Entry32>,
    entry_id: u64,
    _ctrl_dev: OwnedFd,
}

impl UblkCtrl {
    /// ublk control device path
    pub const CTRL_DEV_PATH: &'static str = "/dev/ublk-control";

    /// Signals to the kernel to provide a device id
    pub const NEW_DEV_ID: u32 = u32::MAX; // interpreted as '-1' by the kernel driver

    pub fn new() -> io::Result<Self> {
        let ring = IoUring::generic_builder().build(32)?;

        let ctrl_dev = OpenOptions::new()
            .read(true)
            .write(true)
            .open(Self::CTRL_DEV_PATH)?;

        ring.submitter().register_files(&[ctrl_dev.as_raw_fd()])?;

        let ctrl = UblkCtrl {
            ring,
            entry_id: 0,
            _ctrl_dev: ctrl_dev.into(),
        };
        Ok(ctrl)
    }

    pub fn get_device_info(&mut self, dev_id: u32) -> io::Result<DeviceInfo> {
        let mut info = WireDevInfo::new();

        // Since we block on the submission `&mut info` will be valid until the submission
        // completes.
        let cmd_data = WireCmd::new(dev_id).buffer(&mut info);
        submit_cmd(self, CtrlOp::GetDevInfo, &cmd_data)?;
        Ok(info.into())
    }

    pub fn add_device(&mut self, dev_id: u32, options: DeviceOptions) -> io::Result<DeviceInfo> {
        let mut info = WireDevInfo::new()
            .dev_id(dev_id) // if after cast `dev_id` < 0, it means requesting a new id
            .max_io_buf_bytes(options.max_io_buf_bytes)
            .nr_hw_queues(options.nr_hw_queues)
            .queue_depth(options.queue_depth)
            .flags(options.flags.bits());

        // The kernel driver fails if info.dev_id != cmd.dev_id
        let cmd_data = WireCmd::new(dev_id).buffer(&mut info);
        submit_cmd(self, CtrlOp::AddDev, &cmd_data)?;
        Ok(info.into())
    }

    pub fn delete_device(&mut self, dev_id: u32) -> io::Result<()> {
        let cmd_data = WireCmd::new(dev_id);
        submit_cmd(self, CtrlOp::DelDev, &cmd_data)?;
        Ok(())
    }

    pub fn stop_device(&mut self, dev_id: u32) -> io::Result<()> {
        let cmd_data = WireCmd::new(dev_id);
        submit_cmd(self, CtrlOp::StopDev, &cmd_data)?;
        Ok(())
    }
}

fn submit_cmd(uc: &mut UblkCtrl, cmd_op: CtrlOp, cmd_data: &WireCmd) -> io::Result<()> {
    uc.entry_id += 1;
    let entry_id = uc.entry_id;

    let cmd = UringCmd80::new(Fixed(0), cmd_op as u32)
        .cmd(cmd_data.as_cmd_data())
        .build()
        .user_data(entry_id);

    unsafe { uc.ring.submission().push(&cmd) }.expect("command enqueued");
    let res = uc.ring.submit_and_wait(1)?;
    assert_eq!(res, 1);

    let mut cq = uc.ring.completion();
    let cqe = cq.next().expect("completed ctrl command");
    assert_eq!(entry_id, cqe.user_data());

    // TODO: Check the res for each command
    // driver: -EINVAL (not IO_URING_F_SQE128), -EPERM (not CAP_SYS_ADMIN), -ENODEV
    // get_dev_info 0 ok, -EINVAL (len < sizeof(dev_info), no device), -EFAULT copy to user memory
    // del_dev 0 ok
    // add_dev 0 ok

    let res = cqe.result();
    if res == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(-res))
    }
}
