use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use std::{collections::HashSet, io::Error as IoError, string::FromUtf8Error};

macro_rules! ensure_buffer_has_space {
    ($buf:expr, $space:expr) => {
        if $buf.len() < $space {
            return Ok(None);
        }
    };
}

#[derive(Debug)]
pub enum ProtocolError {
    UnknownFrame,
    UnknownCommand,
    InvalidUtf8,
    InvalidParameterCount,
    Io(IoError),
}

impl From<IoError> for ProtocolError {
    fn from(err: IoError) -> Self {
        Self::Io(err)
    }
}

impl From<FromUtf8Error> for ProtocolError {
    fn from(_: FromUtf8Error) -> Self {
        Self::InvalidUtf8
    }
}

#[derive(Debug)]
pub enum Frame {
    Message(MessageFrame),
    Command(CommandFrame),
}

impl Frame {
    pub fn frame_type(&self) -> FrameType {
        match self {
            Self::Message(_) => FrameType::Message,
            Self::Command(_) => FrameType::Command,
        }
    }
}

#[derive(PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Message = 1,
    Command = 2,
}

impl TryFrom<u8> for FrameType {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Message),
            2 => Ok(Self::Command),
            _ => Err(ProtocolError::UnknownFrame),
        }
    }
}

#[derive(Debug)]
pub struct MessageFrame {
    pub tags: HashSet<String>,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub struct CommandFrame {
    pub command: Command,
    pub parameters: Vec<String>,
}

#[derive(Debug)]
#[repr(u8)]
pub enum Command {
    Subscribe = 1,
    Unsubscribe = 2,
    Ping = 3,
    Pong = 4,
}

impl Command {
    pub fn parameter_bounds(&self) -> (u8, u8) {
        match self {
            Self::Subscribe | Self::Unsubscribe => (0, 255),
            Self::Ping | Self::Pong => (0, 0),
        }
    }
}

impl TryFrom<u8> for Command {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Subscribe),
            2 => Ok(Self::Unsubscribe),
            3 => Ok(Self::Ping),
            4 => Ok(Self::Pong),
            _ => Err(ProtocolError::UnknownCommand),
        }
    }
}

pub struct QueQueCodec {}

impl QueQueCodec {
    pub fn new() -> Self {
        Self {}
    }
}

impl Decoder for QueQueCodec {
    type Item = Frame;
    type Error = ProtocolError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        ensure_buffer_has_space!(src, 1);
        let frame_type = FrameType::try_from(src[0])?;

        if frame_type == FrameType::Command {
            // The frame is a command
            // Parts: command type as byte, argument count as byte, arguments as strings with length as byte
            ensure_buffer_has_space!(src, 3);
            let command_type = Command::try_from(src[1])?;
            let parameter_count = src[2];

            let (min, max) = command_type.parameter_bounds();
            if !(min <= parameter_count && parameter_count <= max) {
                return Err(ProtocolError::InvalidParameterCount);
            }

            let mut parameters = Vec::with_capacity(parameter_count.into());

            src.advance(3);

            for _ in 0..parameter_count {
                ensure_buffer_has_space!(src, 1);
                let string_len = src[0] as usize;
                ensure_buffer_has_space!(src, 1 + string_len);
                let string_raw = src[1..(1 + string_len)].to_vec();
                let string = String::from_utf8(string_raw)?;

                parameters.push(string);

                src.advance(1 + string_len);
            }

            let frame = CommandFrame {
                command: command_type,
                parameters,
            };

            Ok(Some(Frame::Command(frame)))
        } else {
            // The frame is a message
            // Parts: tag length as byte, tags as strings with length as byte, body length as u32, body
            ensure_buffer_has_space!(src, 2);
            let tag_count = src[1] as usize;
            let mut tags = HashSet::with_capacity(tag_count);

            src.advance(2);

            for _ in 0..tag_count {
                ensure_buffer_has_space!(src, 1);
                let string_len = src[0] as usize;
                ensure_buffer_has_space!(src, 1 + string_len);
                let string_raw = src[1..(1 + string_len)].to_vec();
                let string = String::from_utf8(string_raw)?;

                tags.insert(string);

                src.advance(1 + string_len);
            }

            ensure_buffer_has_space!(src, 4);
            let body_length = u32::from_be_bytes(src[0..4].try_into().unwrap()) as usize;
            ensure_buffer_has_space!(src, 4 + body_length);
            let body = src[4..4 + body_length].to_vec();

            let frame = MessageFrame { tags, body };

            Ok(Some(Frame::Message(frame)))
        }
    }
}

impl Encoder<Frame> for QueQueCodec {
    type Error = ProtocolError;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.put_u8(item.frame_type() as u8);

        if let Frame::Command(command) = item {
            dst.put_u8(command.command as u8);
            dst.put_u8(command.parameters.len() as u8);

            for parameter in command.parameters {
                dst.put_u8(parameter.len() as u8);
                dst.extend_from_slice(parameter.as_bytes());
            }
        } else if let Frame::Message(message) = item {
            dst.put_u8(message.tags.len() as u8);

            for tag in message.tags {
                dst.put_u8(tag.len() as u8);
                dst.extend_from_slice(tag.as_bytes());
            }

            dst.put_u32(message.body.len() as u32);
            dst.extend_from_slice(message.body.as_slice());
        }

        Ok(())
    }
}

#[test]
fn test_decode_command() -> Result<(), ProtocolError> {
    let mut codec = QueQueCodec::new();
    let mut input = BytesMut::new();

    let ping = &[2, 3, 0];
    let pong = &[2, 4, 0];
    let subscribe = &[
        2, 1, 1, 11, 104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100,
    ];
    let unsubscribe = &[
        2, 1, 2, 11, 104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100, 3, 121, 101, 115,
    ];

    input.extend_from_slice(ping);
    let maybe_frame = codec.decode(&mut input);
    assert!(matches!(maybe_frame, Ok(Some(_))));

    input.extend_from_slice(pong);
    let maybe_frame = codec.decode(&mut input);
    assert!(matches!(maybe_frame, Ok(Some(_))));

    input.extend_from_slice(subscribe);
    let maybe_frame = codec.decode(&mut input);
    assert!(matches!(maybe_frame, Ok(Some(_))));

    input.extend_from_slice(unsubscribe);
    let maybe_frame = codec.decode(&mut input);
    assert!(matches!(maybe_frame, Ok(Some(_))));

    Ok(())
}

#[test]
fn test_encode_command() -> Result<(), ProtocolError> {
    let mut codec = QueQueCodec::new();
    let mut output = BytesMut::new();

    let ping = Frame::Command(CommandFrame {
        command: Command::Ping,
        parameters: Vec::new(),
    });
    let pong = Frame::Command(CommandFrame {
        command: Command::Pong,
        parameters: Vec::new(),
    });
    let subscribe = Frame::Command(CommandFrame {
        command: Command::Subscribe,
        parameters: vec![String::from("hello world")],
    });
    let unsubscribe = Frame::Command(CommandFrame {
        command: Command::Unsubscribe,
        parameters: vec![String::from("hello world"), String::from("yes")],
    });

    let all_frames = vec![ping, pong, subscribe, unsubscribe];

    for frame in all_frames {
        assert!(codec.encode(frame, &mut output).is_ok());

        let maybe_frame = codec.decode(&mut output);
        assert!(matches!(maybe_frame, Ok(Some(_))));
    }

    Ok(())
}
