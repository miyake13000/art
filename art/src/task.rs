use futures::{future::BoxFuture, task::ArcWake};
use std::sync::{Arc, Mutex, mpsc::SyncSender};

pub(crate) struct Task {
    // 実行するコルーチン
    pub(crate) future: Mutex<BoxFuture<'static, ()>>,
    // Executorへスケジューリングするためのチャネル
    pub(crate) sender: SyncSender<Arc<Task>>,
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        // 自身をスケジューリング
        let self0 = arc_self.clone();
        arc_self.sender.send(self0).unwrap();
    }
}
