// SPDX-License-Identifier: MIT

use clap::Args;
use std::process;
use ublk::control::UblkCtrl;

const MAX_NR_UBLK_DEVS: u32 = 128;

#[derive(Args)]
pub(crate) struct Opt {
    /// ublk device id [default: all ublk devices]
    #[clap(long)]
    device_id: Option<u32>,
}

pub(crate) fn remove_dev(opt: &Opt) {
    let mut ubctrl = UblkCtrl::new().unwrap_or_else(|err| {
        eprintln!("{}", err);
        process::exit(1);
    });
    if let Some(dev_id) = opt.device_id {
        if let Err(err) = ubctrl.delete_device(dev_id) {
            eprintln!("Error device ID {}: {}", dev_id, err);
        }
    } else {
        for dev_id in 0..MAX_NR_UBLK_DEVS {
            let _ = ubctrl.delete_device(dev_id);
        }
    }
}
