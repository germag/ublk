// SPDX-License-Identifier: MIT

extern crate structopt;
extern crate ublk;

use std::io;
use structopt::StructOpt;
use ublk::control::{DeviceFlags, DeviceInfo, DeviceOptions, UblkCtrl};

#[derive(StructOpt)]
#[structopt(name = "adddev", about = "Add a new ublk device.")]
struct Opt {
    /// ublk device id [default: first available id]
    #[structopt(long)]
    device_id: Option<u32>,

    /// Number of hardware queues
    #[structopt(long)]
    num_queues: Option<u16>,

    #[structopt(long)]
    queue_depth: Option<u16>,

    #[structopt(long)]
    max_io_buf_size: Option<u32>,

    #[structopt(long)]
    zero_copy: bool,

    #[structopt(long)]
    iou_comp_in_task: bool,

    #[structopt(long)]
    need_get_data: bool,
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();

    let mut ubctrl = UblkCtrl::new()?;
    let dev_id = if let Some(dev_id) = opt.device_id {
        dev_id
    } else {
        UblkCtrl::NEW_DEV_ID
    };

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

    let options = DeviceOptions::new()
        .nr_hw_queues(num_queues)
        .queue_depth(queue_depth)
        .max_io_buf_bytes(max_io_buf_size)
        .flags(flags);

    match ubctrl.add_device(dev_id, options) {
        Ok(info) => println!("New Device:\n{}\n", dev_info_pprint(info)),
        Err(err) => eprintln!("Error adding device: {}", err),
    }

    Ok(())
}

fn dev_info_pprint(info: DeviceInfo) -> String {
    format!("Device ID: {}\nServer PID: {}\nState: {}\nNr. HW Queues: {}\nQueue depth: {}\nMax IO Buf: {} bytes\nflags: {:?}",
            info.dev_id, info.srv_pid, info.state, info.nr_hw_queues, info.queue_depth, info.max_io_buf_bytes, info.flags)
}
