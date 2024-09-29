use crc::{Crc, CRC_32_ISCSI};
use dimmer_communication::ClientCommandResult;
use postcard::from_bytes_crc32;
use crate::communication::BufferWriter;
use crate::errors::Error;
use crate::pwm_service::PwmChannels;
use crate::storage::Storage;

const MAX_PROCESSED_IDS: usize = 256;

pub struct Server {
    processed_ids: [u32; MAX_PROCESSED_IDS],
    processed_count: u8,
    processed_overflow: bool,
}

impl Server {
    pub fn new(storage: &'static Storage, channels: &'static PwmChannels) -> Self {
        Server {
            processed_ids: [0; MAX_PROCESSED_IDS],
            processed_count: 0,
            processed_overflow: false,
        }
    }

    pub fn idle(
        &mut self, 
        input_data: &[u8],
        out_buffer: &mut dyn BufferWriter, 
        storage: &mut Storage, 
        channels: &mut PwmChannels
    ) -> Result<(), Error> {
        let crc = Crc::<u32>::new(&CRC_32_ISCSI);
        let command:ClientCommandResult = from_bytes_crc32(input_data, crc.digest()).map_err(|_| Error::SerializedDataError)?;
        
        Ok(())
    }
}