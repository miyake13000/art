use art::{net::*, Runtime};
use clap::Parser;
use std::net::Ipv4Addr;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = false)]
    use_sched: bool,
}

fn main() {
    let arg = Args::parse();

    let runtime = Runtime::new(arg.use_sched);
    let spawner = runtime.get_spawner();

    runtime.spawn(async move {
        // 非同期アクセプト用のリスナを生成
        let addr = (Ipv4Addr::new(127, 0, 0, 1), 8000);
        let listener = TcpListener::listen(addr).unwrap();
        println!("Server starts on: {}:{}", addr.0, addr.1);
        loop {
            // 非同期コネクションアクセプト
            let (mut stream, addr) = listener.accept().await.unwrap();
            println!("accept: {}", addr);

            // コネクションごとにタスクを生成
            spawner.spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(size) = stream.read(&mut buf).await {
                    if size == 0 {
                        break;
                    }
                    let write_size = stream.write(&buf[..size]).await.unwrap();
                    println!("echo: {addr}, {write_size} bytes");
                }
                println!("close: {}", addr);
            });
        }
    });

    runtime.run();
}
