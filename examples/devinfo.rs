// SPDX-License-Identifier: MIT

extern crate structopt;
extern crate ublk;

use std::io;
use structopt::StructOpt;
use ublk::control::{DeviceInfo, UblkCtrl};

const MAX_NR_UBLK_DEVS: u32 = 128;

#[derive(StructOpt)]
#[structopt(name = "devinfo", about = "Show ublk device info.")]
struct Opt {
    /// ublk device id [default: all ublk devices]
    #[structopt(long)]
    device_id: Option<u32>,
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();

    let mut ubctrl = UblkCtrl::new()?;
    if let Some(dev_id) = opt.device_id {
        match ubctrl.get_device_info(dev_id) {
            Ok(info) => println!("Device Info:\n{}\n", dev_info_pprint(info)),
            Err(err) => eprintln!("Error device ID {}: {}", dev_id, err),
        }
    } else {
        for dev_id in 0..MAX_NR_UBLK_DEVS {
            if let Ok(info) = ubctrl.get_device_info(dev_id) {
                println!("Device Info:\n{}\n", dev_info_pprint(info));
            }
        }
    }
    Ok(())
}

fn dev_info_pprint(info: DeviceInfo) -> String {
    format!("Device ID: {}\nServer PID: {}\nState: {}\nNr. HW Queues: {}\nQueue depth: {}\nMax IO Buf: {} bytes\nflags: {:?}",
            info.dev_id, info.srv_pid, info.state, info.nr_hw_queues, info.queue_depth, info.max_io_buf_bytes, info.flags)
}
