use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, Stream, StreamExt,
};
use tokio::{
    net::TcpStream,
    sync::mpsc::{error::SendError, unbounded_channel, UnboundedReceiver, UnboundedSender},
};
use tokio_util::codec::{Decoder, Framed};

use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    codec::{Frame, ProtocolError, QueQueCodec},
    error::Error,
};

pub struct QueQueClient {
    sender: QueQueSender,
    receiver: QueQueReceiver,
}

impl QueQueClient {
    pub async fn connect(addr: SocketAddr) -> Result<Self, Error> {
        let tcp = TcpStream::connect(addr).await?;
        let codec = QueQueCodec::new();
        let framed = codec.framed(tcp);

        let (sink, stream) = framed.split();
        let (tx, rx) = unbounded_channel();

        tokio::spawn(send_task(sink, rx));

        let receiver = QueQueReceiver { stream };
        let sender = QueQueSender { sender: tx };

        Ok(Self { sender, receiver })
    }

    pub fn send(&self, frame: Frame) -> Result<(), SendError<Frame>> {
        self.sender.send(frame)
    }

    pub fn into_split(self) -> (QueQueSender, QueQueReceiver) {
        (self.sender, self.receiver)
    }
}

#[derive(Clone)]
pub struct QueQueSender {
    sender: UnboundedSender<Frame>,
}

impl QueQueSender {
    pub fn send(&self, frame: Frame) -> Result<(), SendError<Frame>> {
        self.sender.send(frame)
    }
}

async fn send_task(
    mut sink: SplitSink<Framed<TcpStream, QueQueCodec>, Frame>,
    mut rx: UnboundedReceiver<Frame>,
) {
    while let Some(frame) = rx.recv().await {
        if sink.send(frame).await.is_err() {
            break;
        }
    }
}

pub struct QueQueReceiver {
    stream: SplitStream<Framed<TcpStream, QueQueCodec>>,
}

impl Stream for QueQueReceiver {
    type Item = Result<Frame, ProtocolError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.stream).poll_next(cx)
    }
}

impl Stream for QueQueClient {
    type Item = Result<Frame, ProtocolError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::{sleep, Duration};

    use crate::codec::{Command, CommandFrame, Frame};

    use super::{Error, QueQueClient, SocketAddr};

    #[tokio::test]
    async fn test_client() -> Result<(), Error> {
        let client = QueQueClient::connect(SocketAddr::from(([0, 0, 0, 0], 3322))).await?;

        client
            .send(Frame::Command(CommandFrame {
                command: Command::Ping,
                parameters: Vec::new(),
            }))
            .unwrap();

        sleep(Duration::from_secs(3)).await;

        Ok(())
    }
}
