use art::{Runtime, net::*};
use std::io::Write;

fn main() {
    let runtime = Runtime::new();
    let spawner = runtime.get_spawner();

    let server = async move {
        // 非同期アクセプト用のリスナを生成
        let addr = (std::net::Ipv4Addr::new(0, 0, 0, 0), 8000);
        let listener = TcpListener::listen(addr);
        loop {
            // 非同期コネクションアクセプト
            let (mut reader, mut writer, addr) = listener.accept().await;
            println!("accept: {}", addr);

            // コネクションごとにタスクを生成
            spawner.spawn(async move {
                // 1行非同期読み込み
                while let Some(buf) = reader.read_line().await {
                    print!("read: {}, {}", addr, buf);
                    writer.write_all(buf.as_bytes()).unwrap();
                    writer.flush().unwrap();
                }
                println!("close: {}", addr);
            });
        }
    };

    // タスクを生成して実行
    runtime.spawn(server);
    runtime.run();
}
