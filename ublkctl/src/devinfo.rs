// SPDX-License-Identifier: MIT

use clap::Args;
use std::process;
use ublk::control::{DeviceInfo, DeviceParams, UblkCtrl};

const MAX_NR_UBLK_DEVS: u32 = 128;

#[derive(Args)]
pub(crate) struct Opt {
    /// ublk device id [default: all ublk devices]
    #[clap(long)]
    device_id: Option<u32>,

    /// Show device parameters
    #[clap(long)]
    params: bool,

    /// Show queues cpu affinity
    #[clap(long)]
    affinity: bool,
}

pub(crate) fn get_dev_info(opt: &Opt) {
    let mut ubctrl = UblkCtrl::new().unwrap_or_else(|err| {
        eprintln!("{}", err);
        process::exit(1);
    });

    if let Some(dev_id) = opt.device_id {
        if let Err(err) = show_dev(&mut ubctrl, dev_id, opt.params, opt.affinity) {
            eprintln!("Error device ID {}: {}", dev_id, err);
        }
    } else {
        for dev_id in 0..MAX_NR_UBLK_DEVS {
            let _ = show_dev(&mut ubctrl, dev_id, opt.params, opt.affinity);
        }
    }
}

fn show_dev(uc: &mut UblkCtrl, dev_id: u32, params: bool, affinity: bool) -> ublk::Result<()> {
    let info = uc.get_device_info(dev_id)?;
    println!("\nDevice Info:");
    println!("============");
    println!("{}\n", dev_info_format(info));

    if params {
        let params = uc.get_device_parameters(dev_id)?;
        println!("--  Parameters:\n{}\n", dev_params_format(params));
    }

    if affinity {
        println!("--  Affinity:");
        let cores = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
        let queues = info.nr_hw_queues;
        for queue in 0..queues {
            let cpu_set = uc.get_queue_affinity(dev_id, queue)?;
            let aff = get_cpu_list(cores, &cpu_set);
            println!("\n\tqueue {} cpus: {:?}", queue, aff);
        }
    }
    Ok(())
}

fn dev_info_format(info: DeviceInfo) -> String {
    format!("\tDevice ID: {}\n\tServer PID: {}\n\tActive: {}\n\tNr. HW Queues: {}\n\tQueue depth: {}\n\tMax IO Buf: {} bytes\n\tflags: {:?}",
            info.dev_id, info.srv_pid, info.active, info.nr_hw_queues, info.queue_depth, info.max_io_buf_bytes, info.flags)
}

fn dev_params_format(p: DeviceParams) -> String {
    let bz = 1 << p.logical_bs_shift;
    let basic = format!("{:?}", p);
    let basic = basic
        .replace("DeviceParams", "")
        .replace("DeviceParamDiscard", "")
        .replace("{", "")
        .replace("}", "")
        .replace(',', "\n\t");

    format!("\t Block size: {}\n\t {}", bz, basic.trim())
}

fn get_cpu_list(cores: i64, cpu_set: &libc::cpu_set_t) -> Vec<u32> {
    let mut set = Vec::with_capacity(cores as usize);
    for cpu in 0..cores {
        if unsafe { libc::CPU_ISSET(cpu as usize, cpu_set) } {
            set.push(cpu as u32);
        }
    }
    set
}
