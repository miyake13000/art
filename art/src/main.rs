use art::{Runtime, net::*};
use std::io::Write;
use std::net::Ipv4Addr;

fn main() {
    let runtime = Runtime::new();
    let spawner = runtime.get_spawner();

    runtime.spawn(async move {
        // 非同期アクセプト用のリスナを生成
        let addr = (Ipv4Addr::new(127, 0, 0, 1), 8000);
        let listener = TcpListener::listen(addr);
        println!("Server starts on: {}:{}", addr.0, addr.1);
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
    });

    runtime.run();
}
