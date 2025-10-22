use sched_art::*;
use std::mem::MaybeUninit;
use anyhow::{Result, Context};

fn main() -> Result<()> {
    let mut object = MaybeUninit::uninit();
    let level = LevelFilter::Info;
    let (scheduler, handler) = Scheduler::init(&mut object, level).context("Failed to initialize Scheduler")?;
    ctrlc::set_handler(move || handler.stop()).unwrap();
    scheduler.run().context("Failed to run Scheduler")?;

    Ok(())
}
