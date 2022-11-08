// SPDX-License-Identifier: MIT

use clap::Args;
use std::process;
use ublk::control::{DeviceFlags, DeviceInfo, DeviceOptions, DeviceParams, UblkCtrl};

#[derive(Args)]
pub(crate) struct Opt {
    /// ublk device id [default: first available id]
    #[clap(long)]
    device_id: Option<u32>,

    /// Number of hardware queues
    #[clap(long)]
    num_queues: Option<u16>,

    #[clap(long)]
    queue_depth: Option<u16>,

    #[clap(long)]
    max_io_buf_size: Option<u32>,

    #[clap(long)]
    zero_copy: bool,

    #[clap(long)]
    iou_comp_in_task: bool,

    #[clap(long)]
    need_get_data: bool,
}

pub(crate) fn add_device(opt: &Opt) {
    let mut ubctrl = UblkCtrl::new().unwrap_or_else(|err| {
        eprintln!("{}", err);
        process::exit(1);
    });

    let num_queues = opt
        .num_queues
        .unwrap_or(DeviceOptions::DEFAULT_NR_HW_QUEUES);
    let queue_depth = opt
        .queue_depth
        .unwrap_or(DeviceOptions::DEFAULT_QUEUE_DEPTH);
    let max_io_buf_size = opt
        .max_io_buf_size
        .unwrap_or(DeviceOptions::DEFAULT_BUF_SIZE);

    let mut flags = DeviceFlags::empty();
    if opt.zero_copy {
        flags |= DeviceFlags::ZeroCopy;
    }

    if opt.iou_comp_in_task {
        flags |= DeviceFlags::ForceIouCmdCompleteInTask;
    }

    if opt.need_get_data {
        flags |= DeviceFlags::NeedGetData
    }

    let mut options = DeviceOptions::new()
        .nr_hw_queues(num_queues)
        .queue_depth(queue_depth)
        .max_io_buf_bytes(max_io_buf_size)
        .flags(flags);

    if let Some(dev_id) = opt.device_id {
        options = options.device_id(dev_id);
    };

    let info = ubctrl.add_device(&options).unwrap_or_else(|err| {
        eprintln!("{}", err);
        process::exit(1);
    });

    println!("New Device:\n{}\n", dev_info_pprint(info));

    // let's add some example parameters
    let dev_size = 250 * 1024 * 1024 * 1024;
    let params = DeviceParams {
        attrs: Default::default(),
        logical_bs_shift: 9,
        physical_bs_shift: 12,
        io_opt_shift: 12,
        io_min_shift: 9,
        max_sectors: info.max_io_buf_bytes >> 9, // dividing by the sector size (512)
        dev_sectors: dev_size >> 9,              // dividing by the sector size (512)
        ..Default::default()
    };

    ubctrl
        .set_device_parameters(info.dev_id, &params)
        .unwrap_or_else(|err| {
            eprintln!("{}", err);
            process::exit(1);
        });
}

fn dev_info_pprint(info: DeviceInfo) -> String {
    format!("Device ID: {}\nServer PID: {}\nActive: {}\nNr. HW Queues: {}\nQueue depth: {}\nMax IO Buf: {} bytes\nflags: {:?}",
            info.dev_id, info.srv_pid, info.active, info.nr_hw_queues, info.queue_depth, info.max_io_buf_bytes, info.flags)
}
