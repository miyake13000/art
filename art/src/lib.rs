pub(crate) mod executor;
pub(crate) mod selector;
pub(crate) mod task;

pub mod net;

use sched_art::SchedulerClient;
use std::sync::{Arc, OnceLock};

static SELECTOR: OnceLock<Arc<selector::IOSelector>> = OnceLock::new();
static SCHEDULER: OnceLock<SchedulerClient> = OnceLock::new();

#[derive(Debug)]
pub struct Runtime {
    executor: executor::Executor,
}

#[allow(clippy::new_without_default)]
impl Runtime {
    pub fn new() -> Self {
        let selector = selector::IOSelector::new();
        SELECTOR
            .set(selector)
            .expect("Selector has already initialized");

        let sched_client = SchedulerClient::new().unwrap();
        SCHEDULER
            .set(sched_client)
            .expect("Scheduler has already initialized");

        Self {
            executor: executor::Executor::new(),
        }
    }

    pub fn get_spawner(&self) -> executor::Spawner {
        self.executor.get_spawner()
    }

    pub fn spawn(&self, future: impl std::future::Future<Output = ()> + 'static + Send) {
        let spawner = self.get_spawner();
        spawner.spawn(future);
    }

    pub fn run(&self) {
        self.executor.run();
    }
}
