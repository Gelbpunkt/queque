use std::net::SocketAddr;

use futures_util::StreamExt;
use tokio::net::TcpStream;
use tokio_util::codec::Decoder;

use crate::{codec::QueQueCodec, error::Error};

pub async fn process_socket(socket: TcpStream, addr: SocketAddr) -> Result<(), Error> {
    let codec = QueQueCodec::new();
    let mut framed = codec.framed(socket);

    while let Some(Ok(msg)) = framed.next().await {
        tracing::info!("{:?}", msg);
    }

    tracing::info!("Client from {} disconnected", addr);

    Ok(())
}
