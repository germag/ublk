// SPDX-License-Identifier: MIT

use crate::control::{DeviceFlags, DeviceInfo};

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub(crate) struct WireDevInfo {
    nr_hw_queues: u16,
    queue_depth: u16,
    state: u16,
    _pad0: u16,
    max_io_buf_bytes: u32,
    dev_id: u32,
    ublksrv_pid: i32,
    _pad1: u32,
    flags: u64, // feature flags
    _unused: [u64; 4], // For libublksrv internal use, invisible to ublk driver
}

impl WireDevInfo {
    // Available feature flags
    // zero copy requires 4k block size, and can remap ublk driver's io
    // request into ublksrv's vm space.
    // Kernel driver is not ready to support zero copy.
    pub(crate) const SUPPORT_ZERO_COPY: u64 = 1 << 0;

    // Force to complete io cmd via io_uring_cmd_complete_in_task so that
    // performance comparison is done easily with using task_work_add.
    pub(crate) const URING_CMD_COMP_IN_TASK: u64 = 1 << 1;

    // User should issue io cmd again for write requests to set io buffer address
    // and copy data from bio vectors to the userspace io buffer.
    // In this mode, task_work is not used.
    pub(crate) const NEED_GET_DATA: u64 = 1 << 2;

    pub(crate) const MAX_BUF_SIZE: u32 = 1024 << 10;
    pub(crate) const MAX_NR_HW_QUEUES: u16 = 32;
    pub(crate) const MAX_QUEUE_DEPTH: u16 = 1024;

    pub(crate) const DEFAULT_BUF_SIZE: u32 = 512 << 10;
    pub(crate) const DEFAULT_NR_HW_QUEUES: u16 = 1;
    pub(crate) const DEFAULT_QUEUE_DEPTH: u16 = 256;

    #[inline]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn dev_id(mut self, dev_id: u32) -> Self {
        self.dev_id = dev_id;
        self
    }

    pub(crate) fn nr_hw_queues(mut self, nr_hw_queues: u16) -> Self {
        self.nr_hw_queues = nr_hw_queues;
        self
    }

    pub(crate) fn queue_depth(mut self, queue_depth: u16) -> Self {
        self.queue_depth = queue_depth;
        self
    }

    pub(crate) fn max_io_buf_bytes(mut self, max_io_buf_bytes: u32) -> Self {
        self.max_io_buf_bytes = max_io_buf_bytes;
        self
    }

    pub(crate) fn flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }
}

impl Into<DeviceInfo> for WireDevInfo {
    fn into(self) -> DeviceInfo {
        DeviceInfo {
            dev_id: self.dev_id,
            srv_pid: self.ublksrv_pid,
            state: self.state.try_into().expect("valid device status"),
            nr_hw_queues: self.nr_hw_queues,
            queue_depth: self.queue_depth,
            max_io_buf_bytes: self.max_io_buf_bytes,
            flags: DeviceFlags::from_bits_truncate(self.flags),
        }
    }
}

// Control command opcodes handled by ublk kernel driver.
#[repr(u32)]
pub(crate) enum CtrlOp {
    GetQueueAffinity = 1,
    GetDevInfo = 2,
    AddDev = 4,
    DelDev = 5,
    StartDev = 6,
    StopDev = 7,
    SetParams = 8,
    GetParams = 9,
}

// Control command data (to be sent into UringCmd80::cmd)
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub(crate) struct WireCmd {
    // destination device
    dev_id: u32,
    // destination queue (unused)
    queue_id: u16,
    // cmd op IN/OUT buffer
    len: u16,
    addr: u64,
    // cmd op inline data
    data: [u64; 2],
}

// Since we initialize the ring with IORING_SETUP_SQE128,
// it supports 80 bytes of arbitrary command data
const IOURING_CMD_DATA_SIZE: usize = 80;
type IoUringCmdData = [u8; IOURING_CMD_DATA_SIZE];

// Special value to indicate that the command is not intended for
// a queue, it'll be interpreted as '(u16)-1' by the kernel driver
const QUEUE_IGNORE_ID: u16 = u16::MAX;

impl WireCmd {
    #[inline]
    pub(crate) fn new(dev_id: u32) -> Self {
        Self {
            dev_id,
            queue_id: QUEUE_IGNORE_ID, // unused, only checked in the AddDev command
            len: 0,
            addr: 0,
            data: [0, 0],
        }
    }

    // It's not completely safe, we need to make sure that
    // &buf is valid until the Entry128 (i.e., sqe) is completed.
    #[inline]
    pub(crate) fn buffer<T>(mut self, buf: &mut T) -> Self {
        self.addr = buf as *mut T as u64;
        self.len = std::mem::size_of::<T>() as u16;
        self
    }

    #[inline]
    pub(crate) fn data0(mut self, data: u64) -> Self {
        self.data = [data, self.data[1]];
        self
    }

    #[inline]
    pub(crate) fn data1(mut self, data: u64) -> Self {
        self.data = [self.data[0], data];
        self
    }

    #[inline]
    pub(crate) fn as_cmd_data(self) -> IoUringCmdData {
        let mut data = [0_u8; IOURING_CMD_DATA_SIZE];
        // SAFETY: `data` is valid for writes and `WireCmd` fits into `data`.
        unsafe {
            data.as_mut_ptr().cast::<WireCmd>().write_unaligned(self);
        }
        data
    }
}

const _: () = assert!(
    std::mem::size_of::<WireCmd>() <= std::mem::size_of::<IoUringCmdData>(),
    "invalid size"
);
