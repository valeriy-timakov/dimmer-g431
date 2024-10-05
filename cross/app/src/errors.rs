use dimmer_communication::CommandError;

#[derive(Debug)]
pub enum Error {
    ChannelNotFound(u8),
    GroupNotFound(u8),
    DutyOverflow(f32),
    DmaBufferOverflow,
    SerialTxBusy,
    SerialTxNotStarted,
    FlashError(stm32g4xx_hal::flash::Error),
    StorageEmpty,
    SerializedDataError(postcard::Error),
}

impl Error {
    pub fn to_command_error(&self) -> dimmer_communication::CommandError {
        match self {
            Error::ChannelNotFound(channel) => CommandError::ChannelNotFound(*channel),
            Error::GroupNotFound(group) => CommandError::GroupNotFound(*group),
            Error::DutyOverflow(duty) => CommandError::DutyOverflow(*duty),
            Error::FlashError(_) => CommandError::FlashError,
            Error::StorageEmpty => CommandError::StorageEmpty,
            _ => CommandError::UnknownError,
        }
    }
}