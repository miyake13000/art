use scx_utils::{scx_ops_open, scx_ops_load, scx_ops_attach};
use std::mem::MaybeUninit;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::fs::create_dir_all;
use std::path::PathBuf;
use thiserror::Error;
use log::{info, SetLoggerError};
use simplelog::{SimpleLogger, Config};
pub use simplelog::LevelFilter;
pub use libbpf_rs::OpenObject;

mod bpf_skel;
use bpf_skel::*;

const MAP_PIN_PATH: &str = "/sys/fs/bpf/sched_ext/art/prior_tasks";

pub struct SchedHandler {
    sender: Sender<()>,
}

impl SchedHandler {
    pub fn stop(&self) {
        self.sender.send(()).unwrap();
    }
}

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Failed to initialize logger")]
    LoggerInitError(#[from] SetLoggerError),
    #[error("Failed to open BPF scheduler: {0}")]
    OpenError(String),
    #[error("Failed to load BPF scheduler: {0}")]
    LoadError(String),
    #[error("Failed to attach BPF scheduler: {0}")]
    AttachError(String),
}

pub struct Scheduler<'a> {
    skel: BpfSkel<'a>,
    receiver: Receiver<()>,
}

impl<'a> Scheduler<'a> {
    pub fn init(open_object: &'a mut MaybeUninit<OpenObject>, level: LevelFilter) -> Result<(Self, SchedHandler), SchedulerError> {
        // Create a channel to listen for termination signals
        let (sender, receiver) = channel();

        // Initialize the logger
        SimpleLogger::init(level, Config::default())?;

        // Initialize and load the BPF scheduler
        let builder = BpfSkelBuilder::default();
        let mut skel = scx_ops_open!(builder, open_object, art_ops, None).unwrap();
        let skel = scx_ops_load!(skel, art_ops, uei).unwrap();

        Ok((
            Self { skel, receiver },
            SchedHandler { sender }
        ))
    }

    pub fn run(mut self) -> Result<(), SchedulerError> {
        // Attach the BPF program to the scheduler
        let _link = scx_ops_attach!(self.skel, art_ops).unwrap();
        info!("Successfully attached");

        // Create and pin for the BPF map
        let pin_path = PathBuf::from(MAP_PIN_PATH);
        create_dir_all(pin_path.parent().unwrap()).unwrap();
        self.skel.maps.prior_tasks.pin(&pin_path).unwrap();
        info!("Pinned BPF Map to \"{MAP_PIN_PATH}\"");

        // Wait for termination signal
        self.receiver.recv().unwrap();

        // Remove the pinned map
        self.skel.maps.prior_tasks.unpin(&pin_path).unwrap();
        info!("Unpinned BPF Map from \"{MAP_PIN_PATH}\"");

        Ok(())
    }
}
