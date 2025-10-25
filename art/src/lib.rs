pub(crate) mod executor;
pub(crate) mod selector;
pub(crate) mod task;

pub mod net;

use std::sync::{Arc, OnceLock};
use sched_art::SchedulerClient;

static SELECTOR: OnceLock<Arc<selector::IOSelector>> = OnceLock::new();

#[derive(Debug)]
pub struct Runtime {
    executor: executor::Executor,
    sched_client: SchedulerClient,
}

#[allow(clippy::new_without_default)]
impl Runtime {
    pub fn new() -> Self {
        let selector = selector::IOSelector::new();
        SELECTOR
            .set(selector)
            .expect("Selector has already initialized");

        let sched_client = SchedulerClient::new().unwrap();

        Self {
            executor: executor::Executor::new(),
            sched_client,
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
        self.sched_client.get_priority().unwrap();
        self.executor.run();
    }
}
