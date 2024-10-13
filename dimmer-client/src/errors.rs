use std::io;
use crate::AppCommand;

#[derive(Debug)]
pub enum Error {
    StdIo(io::Error), 
    Io(serialport::Error),
    NoData(String), 
    NumberFormat(std::num::ParseIntError),
    PipelineSendError(AppCommand),
    TokioJoinError(tokio::task::JoinError),
    SerialPortError(serialport::Error), 
    NoSerialConnection, 
    RecvError(tokio::sync::oneshot::error::RecvError),
}
