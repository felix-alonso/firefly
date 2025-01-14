#![feature(alloc_layout_extra)]
#![feature(termination_trait_lib)]
#![feature(process_exitcode_placeholder)]
#![feature(thread_local)]
#![feature(core_intrinsics)]
#![feature(c_unwind)]
#![feature(once_cell)]

#[cfg(not(all(unix, any(target_arch = "x86_64", target_arch = "aarch64"))))]
compile_error!("lumen_rt_minimal does not currently support this architecture!");

extern crate liblumen_crt;

#[macro_use]
mod macros;
mod builtins;
mod config;
pub mod env;
mod logging;
pub mod process;
pub mod scheduler;
pub mod sys;

use liblumen_alloc::erts::process::alloc::default_heap_size;

pub use lumen_rt_core::{
    base, binary_to_string, context, distribution, integer_to_string, proplist, registry, send,
    time, timer,
};

use anyhow::anyhow;
use bus::Bus;
use log::Level;

use self::config::Config;
use self::sys::break_handler::{self, Signal};

#[liblumen_core::entry]
fn main() -> i32 {
    use std::process::Termination;

    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");
    main_internal(name, version, Vec::new()).report().to_i32()
}

fn main_internal(name: &str, version: &str, argv: Vec<String>) -> anyhow::Result<()> {
    self::env::init_argv_from_slice(std::env::args_os()).unwrap();
    // Load system configuration
    let _config = match Config::from_argv(name.to_string(), version.to_string(), argv) {
        Ok(config) => config,
        Err(err) => {
            return Err(anyhow!(err));
        }
    };

    // This bus is used to receive signals across threads in the system
    let mut bus: Bus<break_handler::Signal> = Bus::new(1);
    // Each thread needs a reader
    let mut rx1 = bus.add_rx();
    // Initialize the break handler with the bus, which will broadcast on it
    break_handler::init(bus);

    // Start logger
    let level_filter = Level::Info.to_level_filter();
    logging::init(level_filter).expect("Unexpected failure initializing logger");

    let scheduler = scheduler::current();
    scheduler.spawn_init(default_heap_size()).unwrap();
    loop {
        // Run the scheduler for a cycle
        let scheduled = scheduler.run_once();
        // Check for system signals, and terminate if needed
        if let Ok(sig) = rx1.try_recv() {
            match sig {
                // For now, SIGINT initiates a controlled shutdown
                Signal::INT => {
                    // If an error occurs, report it before shutdown
                    if let Err(err) = scheduler.shutdown() {
                        return Err(anyhow!(err));
                    } else {
                        break;
                    }
                }
                // Technically, we may never see these signals directly,
                // we may just be terminated out of hand; but just in case,
                // we handle them explicitly by immediately terminating, so
                // that we are good citizens of the operating system
                sig if sig.should_terminate() => {
                    return Ok(());
                }
                // All other signals can be surfaced to other parts of the
                // system for custom use, e.g. SIGCHLD, SIGALRM, SIGUSR1/2
                _ => (),
            }
        }
        // If the scheduler scheduled a process this cycle, then we're busy
        // and should keep working until we have an idle period
        if scheduled {
            continue;
        }

        break;
    }

    match scheduler.shutdown() {
        Ok(_) => Ok(()),
        Err(err) => Err(anyhow!(err)),
    }
}
