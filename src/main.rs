use clap::{Parser, Subcommand};
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    cmd: Commands,
}

#[derive(Debug, Default)]
struct Client {
    id: u8,
    mode: u8, // 0: sub, 1: main
    tid: u8,  // target id
    socket: Option<TcpStream>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Main {
        // 127.0.0.1:4321
        server_addr: String,
        // 127.0.0.1:3389
        real_server: String,
    },
    Sub {
        // 127.0.0.1:4321
        server_addr: String,
        // 127.0.0.1:4444
        bind_addr: String,
    },
    Server {
        // 0.0.0.0:4321
        listen_addr: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Args::parse();
    dbg!(&cli);
    match &cli.cmd {
        Commands::Main {
            server_addr,
            real_server,
        } => {
            println!(
                "[*] Main, server_addr:{}, real_server:{}",
                server_addr, real_server
            );
            client_main(server_addr, real_server).await?;
        }
        Commands::Sub {
            server_addr,
            bind_addr,
        } => {
            println!(
                "[*] Sub, server_addr:{}, bind_addr:{}",
                server_addr, bind_addr
            );
            client_sub(server_addr, bind_addr).await?;
        }
        Commands::Server { listen_addr } => {
            println!("[*] server listen at {}", listen_addr);
            server(listen_addr).await?;
        }
    }
    Ok(())
}

async fn handshake(
    client: &mut Client,
    socket: &mut TcpStream,
) -> Result<(), Box<dyn std::error::Error>> {
    client.id = socket.read_u8().await?;
    client.mode = socket.read_u8().await?;
    client.tid = socket.read_u8().await?;
    Ok(())
}
async fn server(listener_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    // let listener_addr = "0.0.0.0:4321";
    let listener = TcpListener::bind(listener_addr).await?;
    let mut g_map: HashMap<u8, Client> = HashMap::new();
    loop {
        let (mut client_socket, _) = listener.accept().await.unwrap();
        let mut client: Client = Default::default();
        match handshake(&mut client, &mut client_socket).await {
            Err(err) => {
                println!("handshake error: {}", err);
                // continue;
            }
            _ => {
                client.socket = Some(client_socket);
            }
        }
        let mode_info = if client.mode == 0 {
            "sub"
        } else if client.mode == 1 {
            "main"
        } else {
            println!("error: unknow mode: 0x{:x}", client.mode);
            panic!("wrong mode")
            // continue;
        };
        println!(
            "server got new comming, id=0x{:x}, mode={}, tid=0x{:x}",
            client.id, mode_info, client.tid
        );

        if client.mode == 0 {
            // sub
            println!("comming a sub");
            let mother = g_map.get_mut(&client.tid);
            let mut c_socket = client.socket.unwrap();
            if mother.is_none() {
                let msg = format!("tid 0x{:x} not exist, wait and retry", client.tid);
                println!("{}", &msg);
                c_socket.write(&[0]).await?;
                c_socket.shutdown().await?;
                continue;
            }
            c_socket.write_u8(1).await?;
            let mother = mother.unwrap();
            println!("found mother by tid=0x{:x}, start copy stream", client.tid);

            // let action: u8 = 0;
            let action = c_socket.read_u8().await?;
            if action == 0x12 {
                // got new connection
                println!("waiting for new connectionm action=0x12");
                let (mut new_conn, _) = listener.accept().await.unwrap();
                println!("got new connectionm action=0x12");
                match tokio::io::copy_bidirectional(
                    &mut new_conn,
                    &mut mother.socket.as_mut().unwrap(),
                )
                .await
                {
                    Ok((to_egress, to_ingress)) => {
                        println!(
                            "server Connection ended gracefully ({} bytes from client, {} bytes from server)",
                            to_egress, to_ingress,
                        );
                    }
                    Err(err) => {
                        println!("server Error while proxying: {}", err);
                        // break;
                    }
                }
            }
            let mid = mother.id;
            drop(mother);
            // mother.socket = None;
            g_map.remove(&mid);
        } else if client.mode == 1 {
            // main
            println!("comming a main");
            // client.socket.unwrap().write("hello main, plz wait for sub".as_bytes());
            let id = client.id;
            g_map.insert(client.id, client);
            let s = g_map.get_mut(&id).unwrap();
            // s.socket.as_mut().unwrap().write_u8(1).await?;
            s.socket.as_mut().unwrap().write(&[1]).await?;
            s.socket.as_mut().unwrap().flush().await?;
        }
    }
    // Ok(())
}

async fn client_sub(server_addr: &str, bind_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(bind_addr).await.unwrap();
    println!("client#1 listening on {} for fake service", bind_addr);
    loop {
        println!("client#1 got local connection");
        let mut remote_conn = TcpStream::connect(server_addr).await.unwrap();
        println!("client#1 connect to {}", server_addr);
        remote_conn.write_all(&[0x30, 0, 0x31]).await?;
        match remote_conn.read_u8().await {
            Ok(ok) => {
                if ok == 0 {
                    println!("not ok, wait 1s");
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    continue;
                }
            }
            Err(_) => {
                println!("not ok");
                return Ok(());
            }
        }
        remote_conn.write_all(&[0x12]).await?;
        let mut new_conn = TcpStream::connect(server_addr).await.unwrap();
        println!("ok, wait for local connect on {}", bind_addr);
        let (mut socket, _) = listener.accept().await.unwrap();
        match tokio::io::copy_bidirectional(&mut new_conn, &mut socket).await {
            Ok((to_egress, to_ingress)) => {
                println!(
                        "client1 Connection ended gracefully ({} bytes from client, {} bytes from server)",
                        to_egress, to_ingress,
                    );
            }
            Err(err) => {
                println!("client1 Error while proxying: {}", err);
            }
        }
    }
    // Ok(())
}

async fn client_main(
    server_addr: &str,
    real_service_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let mut remote_conn = TcpStream::connect(server_addr).await.unwrap();
        println!("client#2 connect to {}", server_addr);
        remote_conn.write_all(&[0x31, 1, 0]).await?;
        match remote_conn.read_u8().await {
            Ok(_) => {
                // return ok;
            }
            Err(_) => {
                println!("not ok");
                return Ok(());
            }
        }
        println!("handshake ok");
        let mut rdp_conn = TcpStream::connect(real_service_addr).await.unwrap();
        println!("client#2 connect to {}", real_service_addr);
        match tokio::io::copy_bidirectional(&mut remote_conn, &mut rdp_conn).await {
            Ok((to_egress, to_ingress)) => {
                println!(
                        "client2 Connection ended gracefully ({} bytes from client, {} bytes from server)",
                        to_egress, to_ingress,
                    );
            }
            Err(err) => {
                println!("client2 Error while proxying: {}", err);
                // break;
            }
        }
    }
    // Ok(())
}
