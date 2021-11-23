use libc::{c_int, sighandler_t, signal, SIGINT, SIGTERM};
use tokio::net::TcpListener;

use std::{env, net::SocketAddr};

use crate::error::Error;

mod client;
mod codec;
mod error;
mod server;

pub extern "C" fn handler(_: c_int) {
    std::process::exit(0);
}

unsafe fn set_os_handlers() {
    signal(SIGINT, handler as extern "C" fn(_) as sighandler_t);
    signal(SIGTERM, handler as extern "C" fn(_) as sighandler_t);
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    unsafe { set_os_handlers() };

    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }

    tracing_subscriber::fmt::init();

    let addr = SocketAddr::from(([0, 0, 0, 0], 3322));
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("Listening on {}", addr);

    loop {
        let (socket, remote_addr) = listener.accept().await?;

        tracing::info!("Connection from {}", remote_addr);

        tokio::spawn(server::process_socket(socket, remote_addr));
    }
}
