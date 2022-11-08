// SPDX-License-Identifier: MIT

use crate::control::{DeviceAttr, DeviceFlags, DeviceInfo, DeviceParamDiscard, DeviceParams};
use io_uring::opcode::UringCmd80;
use io_uring::types::Fixed;
use io_uring::{cqueue, squeue, IoUring};
use std::marker::PhantomData;
use std::{io, mem};

// Control command opcodes handled by ublk kernel driver.
#[repr(u32)]
#[derive(Debug, Copy, Clone)]
pub enum CtrlOp {
    GetQueueAffinity = 1,
    GetDevInfo = 2,
    AddDev = 4,
    DelDev = 5,
    StartDev = 6,
    StopDev = 7,
    SetParams = 8,
    GetParams = 9,
}

// Since we initialize the ring with IORING_SETUP_SQE128,
// it supports 80 bytes of arbitrary command data
const IOURING_CMD_DATA_SIZE: usize = 80;
type IoUringCmdData = [u8; IOURING_CMD_DATA_SIZE];

// Control command data (to be sent into UringCmd80::cmd)
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct CmdData {
    // destination device
    dev_id: u32,
    // destination queue (unused)
    _queue_id: u16,
    // cmd op IN/OUT buffer
    len: u16,
    addr: u64,
    // cmd op inline data
    data: [u64; 2],
}

const _: () = assert!(
    mem::size_of::<CmdData>() <= mem::size_of::<IoUringCmdData>(),
    "invalid size"
);

impl From<CmdData> for IoUringCmdData {
    fn from(cmd_data: CmdData) -> Self {
        let mut data = [0_u8; IOURING_CMD_DATA_SIZE];
        // SAFETY: `data` is valid for writes and `CmdData` fits into `data`.
        unsafe {
            data.as_mut_ptr()
                .cast::<CmdData>()
                .write_unaligned(cmd_data);
        }
        data
    }
}

// Control command
// It uses the standard Rust lifetime specification to make most use-after-free
// errors fail to compile. The `CtrlCmd` is pinned to the lifetime of the
// backing buffer (if any) so the following won't compile:
//
// ```
//  let mut info = DevInfo::new();
//  let cmd = CtrlCmd::new(CtrlOp::GetDevInfo, 0).buffer(&mut info);
//
//  drop(info);
//  cmd.submit_and_wait(uniq, &mut ring);
// ```
#[derive(Debug, Copy, Clone)]
pub struct CtrlCmd<'a> {
    op: CtrlOp,
    lifetime: PhantomData<&'a mut ()>,
    cmd_data: CmdData,
}

impl<'a> CtrlCmd<'a> {
    // Special value to indicate that the command is not intended for
    // a queue, it'll be interpreted as '(u16)-1' by the kernel driver
    const QUEUE_IGNORE_ID: u16 = u16::MAX;

    #[inline]
    pub fn new(op: CtrlOp, dev_id: u32) -> Self {
        Self {
            op,
            lifetime: PhantomData,
            cmd_data: CmdData {
                dev_id,
                _queue_id: Self::QUEUE_IGNORE_ID, // unused, only checked in the AddDev command
                len: 0,
                addr: 0,
                data: [0, 0], // data[1] is unused
            },
        }
    }

    #[inline]
    pub fn buffer<T>(mut self, buf: &'a mut T) -> Self {
        self.cmd_data.addr = buf as *mut T as u64;
        self.cmd_data.len = mem::size_of::<T>() as u16;
        self
    }

    #[inline]
    pub fn data(mut self, data: u64) -> Self {
        // data[1] is unused, should we rename this method
        // to data0() and add data1()?
        self.cmd_data.data = [data, 0];
        self
    }

    pub fn submit_and_wait(
        &self,
        uniq: u64,
        ring: &mut IoUring<squeue::Entry128, cqueue::Entry32>,
    ) -> crate::Result<()> {
        let cmd = UringCmd80::new(Fixed(0), self.op as u32)
            .cmd(self.cmd_data.into())
            .build()
            .user_data(uniq);

        // SAFETY: Since we block on the submission the command buffer will be valid
        // until the submission completes.
        unsafe { ring.submission().push(&cmd) }?;
        let res = ring.submit_and_wait(1)?;
        assert_eq!(res, 1);

        let mut cq = ring.completion();
        let cqe = cq.next().expect("completed ctrl command");
        assert_eq!(uniq, cqe.user_data());

        let res = cqe.result();
        if res == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(-res).into())
        }
    }
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct DevInfo {
    nr_hw_queues: u16,
    queue_depth: u16,
    state: u16,
    _pad0: u16,
    max_io_buf_bytes: u32,
    dev_id: u32,
    ublksrv_pid: i32,
    _pad1: u32,
    flags: u64,        // feature flags
    _unused: [u64; 4], // For libublksrv internal use, invisible to ublk driver
}

impl DevInfo {
    // Signals to the kernel to provide a device id
    pub const NEW_DEV_ID: u32 = u32::MAX; // interpreted as '-1' by the kernel driver

    // Device state
    #[allow(unused)]
    const STATE_DEV_DEAD: u16 = 0;
    const STATE_DEV_LIVE: u16 = 1;

    // Available feature flags
    // zero copy requires 4k block size, and can remap ublk driver's io
    // request into ublksrv's vm space.
    // Kernel driver is not ready to support zero copy.
    pub const SUPPORT_ZERO_COPY: u64 = 1 << 0;

    // Force to complete io cmd via io_uring_cmd_complete_in_task so that
    // performance comparison is done easily with using task_work_add.
    pub const URING_CMD_COMP_IN_TASK: u64 = 1 << 1;

    // User should issue io cmd again for write requests to set io buffer address
    // and copy data from bio vectors to the userspace io buffer.
    // In this mode, task_work is not used.
    pub const NEED_GET_DATA: u64 = 1 << 2;

    pub const MAX_BUF_SIZE: u32 = 1024 << 10;
    pub const MAX_NR_HW_QUEUES: u16 = 32;
    pub const MAX_QUEUE_DEPTH: u16 = 1024;

    pub const DEFAULT_BUF_SIZE: u32 = 512 << 10;
    pub const DEFAULT_NR_HW_QUEUES: u16 = 1;
    pub const DEFAULT_QUEUE_DEPTH: u16 = 256;

    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn dev_id(mut self, dev_id: u32) -> Self {
        self.dev_id = dev_id;
        self
    }

    pub const fn nr_hw_queues(mut self, nr_hw_queues: u16) -> Self {
        self.nr_hw_queues = nr_hw_queues;
        self
    }

    pub const fn queue_depth(mut self, queue_depth: u16) -> Self {
        self.queue_depth = queue_depth;
        self
    }

    pub const fn max_io_buf_bytes(mut self, max_io_buf_bytes: u32) -> Self {
        self.max_io_buf_bytes = max_io_buf_bytes;
        self
    }

    pub const fn flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }
}

impl From<DevInfo> for DeviceInfo {
    fn from(info: DevInfo) -> Self {
        Self {
            dev_id: info.dev_id,
            srv_pid: info.ublksrv_pid,
            active: info.state == DevInfo::STATE_DEV_LIVE,
            nr_hw_queues: info.nr_hw_queues,
            queue_depth: info.queue_depth,
            max_io_buf_bytes: info.max_io_buf_bytes,
            flags: DeviceFlags::from_bits_truncate(info.flags),
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DevParams {
    // Total length of parameters, userspace has to set 'len' for both
    // SET_PARAMS and GET_PARAMS command, and driver may update len  if two
    // sides use different version of 'ublk_params', same with 'types' fields.
    len: u32,
    // types of parameter included (only checked on SetParams).
    types: u32,

    basic: DevParamBasic,
    discard: DevParamDiscard,
}

impl DevParams {
    // Available DevParams::types flags
    const TYPE_BASIC: u32 = 1 << 0; // mandatory on SetParams
    const TYPE_DISCARD: u32 = 1 << 1; // optional

    // Only used in GetParams
    pub fn empty() -> Self {
        // The kernel doesn't check the if flags are set,
        // it'll return all available parameters.
        Self {
            len: mem::size_of::<Self>() as u32,
            types: 0,
            basic: DevParamBasic::default(),
            discard: DevParamDiscard::default(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct DevParamBasic {
    attrs: u32,
    logical_bs_shift: u8,
    physical_bs_shift: u8,
    io_opt_shift: u8,
    io_min_shift: u8,
    max_sectors: u32,
    chunk_sectors: u32,
    dev_sectors: u64,
    virt_boundary_mask: u64,
}

impl DevParamBasic {
    // Available DeviceParamBasic::attrs flags
    pub const ATTR_READ_ONLY: u32 = 1 << 0;
    pub const ATTR_ROTATIONAL: u32 = 1 << 1;
    pub const ATTR_VOLATILE_CACHE: u32 = 1 << 2;
    pub const ATTR_FUA: u32 = 1 << 3;
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct DevParamDiscard {
    discard_alignment: u32,
    discard_granularity: u32,
    max_discard_sectors: u32,
    max_write_zeroes_sectors: u32,
    max_discard_segments: u16,
    _reserved0: u16,
}

impl From<DevParams> for DeviceParams {
    fn from(p: DevParams) -> Self {
        let discard = ((p.types & DevParams::TYPE_DISCARD) != 0).then_some(DeviceParamDiscard {
            discard_alignment: p.discard.discard_alignment,
            discard_granularity: p.discard.discard_granularity,
            max_discard_sectors: p.discard.max_discard_sectors,
            max_write_zeroes_sectors: p.discard.max_write_zeroes_sectors,
            max_discard_segments: p.discard.max_discard_segments,
        });

        Self {
            attrs: DeviceAttr::from_bits_truncate(p.basic.attrs),
            logical_bs_shift: p.basic.logical_bs_shift,
            physical_bs_shift: p.basic.physical_bs_shift,
            io_opt_shift: p.basic.io_opt_shift,
            io_min_shift: p.basic.io_min_shift,
            max_sectors: p.basic.max_sectors,
            chunk_sectors: p.basic.chunk_sectors,
            dev_sectors: p.basic.dev_sectors,
            virt_boundary_mask: p.basic.virt_boundary_mask,
            discard,
        }
    }
}

impl From<&DeviceParams> for DevParams {
    fn from(d: &DeviceParams) -> Self {
        let mut p = Self::empty();
        p.types = Self::TYPE_BASIC;

        p.basic.attrs = d.attrs.bits();
        p.basic.logical_bs_shift = d.logical_bs_shift;
        p.basic.physical_bs_shift = d.physical_bs_shift;
        p.basic.io_opt_shift = d.io_opt_shift;
        p.basic.io_min_shift = d.io_min_shift;
        p.basic.max_sectors = d.max_sectors;
        p.basic.chunk_sectors = d.chunk_sectors;
        p.basic.dev_sectors = d.dev_sectors;
        p.basic.virt_boundary_mask = d.virt_boundary_mask;

        p.discard = d.discard.map_or_else(DevParamDiscard::default, |discard| {
            p.types |= Self::TYPE_DISCARD;
            discard.into()
        });

        p
    }
}

impl From<DeviceParamDiscard> for DevParamDiscard {
    fn from(p: DeviceParamDiscard) -> Self {
        Self {
            discard_alignment: p.discard_alignment,
            discard_granularity: p.discard_granularity,
            max_discard_sectors: p.max_discard_sectors,
            max_write_zeroes_sectors: p.max_write_zeroes_sectors,
            max_discard_segments: p.max_discard_segments,
            _reserved0: 0,
        }
    }
}
