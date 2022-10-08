// SPDX-License-Identifier: MIT

extern crate structopt;
extern crate ublk;

use std::io;
use structopt::StructOpt;
use ublk::control::UblkCtrl;

const MAX_NR_UBLK_DEVS: u32 = 128;

#[derive(StructOpt)]
#[structopt(name = "rmdev", about = "Remove ublk devices.")]
struct Opt {
    /// ublk device id [default: all ublk devices]
    #[structopt(long)]
    device_id: Option<u32>,
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();

    let mut ubctrl = UblkCtrl::new()?;
    if let Some(dev_id) = opt.device_id {
        if let Err(err) = ubctrl.delete_device(dev_id) {
            eprintln!("Error device ID {}: {}", dev_id, err);
        }
    } else {
        for dev_id in 0..MAX_NR_UBLK_DEVS {
            let _ = ubctrl.delete_device(dev_id);
        }
    }
    Ok(())
}
