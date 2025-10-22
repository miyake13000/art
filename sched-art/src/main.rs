use scx_utils::{scx_ops_open, scx_ops_load, scx_ops_attach};
use std::mem::MaybeUninit;
use libbpf_rs::OpenObject;
use std::sync::mpsc::channel;
use std::fs::create_dir_all;
use std::path::PathBuf;

mod bpf_skel;
use bpf_skel::*;

const MAP_PIN_PATH: &str = "/sys/fs/bpf/sched_ext/art/prior_tasks";

fn main() {
    let (sender, receiver) = channel();
    ctrlc::set_handler(move || sender.send(()).unwrap()).unwrap();

    let builder = BpfSkelBuilder::default();
    let mut open_object: MaybeUninit<OpenObject> = MaybeUninit::uninit();

    let mut skel = scx_ops_open!(builder, &mut open_object, art_ops, None).unwrap();
    let mut skel = scx_ops_load!(skel, art_ops, uei).unwrap();
    let _link = scx_ops_attach!(skel, art_ops).unwrap();
    println!("[sched-art] Successfully loaded");

    let pin_path = PathBuf::from(MAP_PIN_PATH);
    create_dir_all(pin_path.parent().unwrap()).unwrap();
    skel.maps.prior_tasks.pin(&pin_path).unwrap();
    println!("[sched-art] Pinned BPF Map to \"{MAP_PIN_PATH}\"");

    receiver.recv().unwrap();
    skel.maps.prior_tasks.unpin(&pin_path).unwrap();
    println!("[sched-art] Unpinned BPF Map from \"{MAP_PIN_PATH}\"");
}
