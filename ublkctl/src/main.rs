use adddev::add_device;
use clap::{Parser, Subcommand};
use devinfo::get_dev_info;
use rmdev::remove_dev;

mod adddev;
mod devinfo;
mod rmdev;

#[derive(Parser)]
#[clap(version, about)]
struct CommandLineArgs {
    /// Verb to run
    #[clap(subcommand)]
    command: CommandLineCommand,
}

#[derive(Subcommand)]
enum CommandLineCommand {
    /// Add a new ublk device
    #[command(name = "add")]
    AddDevice(adddev::Opt),

    /// Remove a ublk device
    #[command(name = "rm")]
    RemoveDevice(rmdev::Opt),

    /// Get ublk device info
    #[command(name = "info")]
    GetDeviceInfo(devinfo::Opt),
}

fn main() {
    let args = CommandLineArgs::parse();

    match args.command {
        CommandLineCommand::AddDevice(o) => add_device(&o),
        CommandLineCommand::RemoveDevice(o) => remove_dev(&o),
        CommandLineCommand::GetDeviceInfo(o) => get_dev_info(&o),
    }
}
