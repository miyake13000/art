use thiserror::Error;
use log::SetLoggerError;

mod client;
pub use client::SchedulerClient;
mod scheduler;
pub use scheduler::Scheduler;

pub(crate) mod bpf_skel;

const MAP_PIN_PATH: &str = "/sys/fs/bpf/sched_ext/art/prior_tasks";

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Failed to initialize logger: {0}")]
    LoggerInitError(#[from] SetLoggerError),
    #[error("Failed to open BPF scheduler: {0}")]
    OpenError(String),
    #[error("Failed to load BPF scheduler: {0}")]
    LoadError(String),
    #[error("Failed to attach BPF scheduler: {0}")]
    AttachError(String),
    #[error("Failed to open BPF Map: {0}")]
    BPFMapOpenError(String),
    #[error("Failed to get priority: {0}")]
    PrioritizeError(#[from] nix::Error),
}
