use libbpf_rs::{MapCore, MapFlags, MapHandle};
use nix::unistd::gettid;
use std::ffi::c_int;

use super::SchedulerError;
use super::MAP_PIN_PATH;

#[derive(Debug)]
pub struct SchedulerClient {
    map: MapHandle,
}

impl SchedulerClient {
    pub fn new() -> Result<Self, SchedulerError> {
        let map = MapHandle::from_pinned_path(MAP_PIN_PATH).unwrap();

        Ok(Self { map })
    }

    pub fn get_priority(&self) -> Result<(), SchedulerError> {
        let key = gettid().as_raw() as c_int;
        let key_bytes = key.to_le_bytes();
        let value = 1u8;
        let value_bytes = value.to_le_bytes();

        self.map
            .update(&key_bytes, &value_bytes, MapFlags::ANY)
            .unwrap();

        Ok(())
    }

    pub fn release_priority(&self) -> Result<(), SchedulerError> {
        let key = gettid().as_raw() as c_int;
        let key_bytes = key.to_le_bytes();

        self.map.delete(&key_bytes).unwrap();

        Ok(())
    }
}
