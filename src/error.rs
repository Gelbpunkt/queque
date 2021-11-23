use std::io::Error as IoError;

use crate::codec::ProtocolError;

#[derive(Debug)]
pub enum Error {
    Protocol(ProtocolError),
    Io(IoError),
}

impl From<ProtocolError> for Error {
    fn from(err: ProtocolError) -> Self {
        Self::Protocol(err)
    }
}

impl From<IoError> for Error {
    fn from(err: IoError) -> Self {
        Self::Io(err)
    }
}
