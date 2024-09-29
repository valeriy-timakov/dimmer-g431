

#[derive(Debug)]
pub enum Error {
    ChannelNotFound(u8),
    DutyOverflow(f32),
    DmaBufferOverflow,
    SerialTxBusy,
    SerialTxNotStarted,
    FlashError(stm32g4xx_hal::flash::Error),
    StorageEmpty,
    SerializedDataError,
}