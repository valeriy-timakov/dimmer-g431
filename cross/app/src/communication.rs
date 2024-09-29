use core::fmt::Write;
use embedded_dma::{ReadBuffer, StaticReadBuffer};
use embedded_hal::digital::v2::{OutputPin, PinState};
use stm32g4xx_hal::dma::{MemoryToPeripheral, Transfer, TransferExt};
use stm32g4xx_hal::dma::traits::{Stream, TargetAddress};
use stm32g4xx_hal::dma::transfer::ConstTransfer;

use crate::errors::Error;
use crate::debug_led::DebugLed;


pub struct TxTransfer<STREAM, PERIPHERAL, BUF, PIN>
    where
        STREAM: Stream + TransferExt<STREAM>,
        PERIPHERAL: TargetAddress<MemoryToPeripheral>,
        <STREAM as Stream>::Config: Clone,
        PIN: OutputPin,
{
    tx_transfer: Option<Transfer<STREAM, PERIPHERAL, MemoryToPeripheral, BUF, ConstTransfer>>,
    raw_data: Option<(STREAM, PERIPHERAL, BUF)>,
    dma_config:  <STREAM as Stream>::Config,
    state_pin: DebugLed<PIN>,
}

impl <STREAM, PERIPHERAL, BUF, PIN> TxTransfer<STREAM, PERIPHERAL, BUF, PIN>
    where
        STREAM: Stream + TransferExt<STREAM>,
        PERIPHERAL: TargetAddress<MemoryToPeripheral>,
        BUF: StaticReadBuffer<Word = <PERIPHERAL as TargetAddress<MemoryToPeripheral>>::MemSize> + BufferWriter,
        <STREAM as Stream>::Config: Clone,
        PIN: OutputPin,
{
    pub fn new(
        stream: STREAM, 
        tx: PERIPHERAL, 
        buff: BUF, 
        dma_config:  <STREAM as Stream>::Config, 
        state_pin: DebugLed<PIN>
    ) -> Self {
        Self {
            tx_transfer: None,
            raw_data: Some((stream, tx, buff)),
            dma_config,
            state_pin, 
        }
    }

    pub fn send<F>(&mut self, writer: F) -> Result<(), Error>
        where
            F: for<'a> FnOnce(&'a mut dyn BufferWriter) -> Result<(), Error>,
    {
        let buffer = self.get_writer()?;
        buffer.clear();
        writer(buffer)?;
        self.start()
    }

    pub fn send_silent<F>(&mut self, writer: F)
        where
            F: for<'a> FnOnce(&'a mut dyn BufferWriter) -> Result<(), Error>,
    {
        let _ = self.send(writer).inspect_err(|_| {
            self.state_pin.set();
        });
    }
    
    #[inline(always)]
    pub fn is_error(&self) -> bool {
        self.state_pin.is_on()
    }

    #[inline(always)]
    pub fn clear_error(&mut self) {
        self.state_pin.clear();
    }

    pub fn get_writer<'a>(&'a mut self) -> Result<&'a mut dyn BufferWriter, Error> {
        match &mut self.raw_data {
            Some((_, _, buffer)) => Ok(buffer),
            None => Err(Error::DmaBufferOverflow),
        }
    }

    pub fn start(&mut self) -> Result<(), Error> {
        match self.raw_data.take() {
            Some((stream, tx, buffer)) => {                
                let mut tx_transfer = stream
                    .into_memory_to_peripheral_transfer(tx, buffer, self.dma_config.clone());
                tx_transfer.start(|_tx| {});
                self.tx_transfer = Some(tx_transfer);
                self.state_pin.on();
                Ok(())
            }
            None => {
                Err(Error::SerialTxBusy)
            }
        }
    }  

    pub fn on_transfer_complete(&mut self) -> Result<(), Error> {
        match self.tx_transfer.as_ref() {
            Some(mut tx_transfer) => {
                if tx_transfer.get_transfer_complete_flag() {
                    let mut tx_transfer = self.tx_transfer.take().unwrap(); 
                    tx_transfer.clear_interrupts();
                    let mut raw_data = tx_transfer.free();
                    raw_data.2.clear();
                    self.raw_data = Some(raw_data);
                    self.state_pin.off();
                    Ok(())
                } else {
                    Err(Error::SerialTxBusy)
                }
            }
            None => {
                Err(Error::SerialTxNotStarted)
            }
        }
    }
    
    pub fn is_busy(&self) -> bool {
        self.tx_transfer.is_some()
    }

    #[inline(always)]
    pub fn tick(&mut self, curr_millis: u64) {
        self.state_pin.tick(curr_millis);
    }
}

impl <STREAM, PERIPHERAL, PIN, const BUFFER_SIZE: usize> TxTransfer<STREAM, PERIPHERAL, Buffer<BUFFER_SIZE>, PIN>
    where
        STREAM: Stream + TransferExt<STREAM>,
        PERIPHERAL: TargetAddress<MemoryToPeripheral>,
        <STREAM as Stream>::Config: Clone,
        PIN: OutputPin,
{
    pub fn new_sb(
        stream: STREAM, 
        tx: PERIPHERAL, 
        buffer: &'static mut [u8; BUFFER_SIZE], 
        dma_config:  <STREAM as Stream>::Config, 
        state_pin: DebugLed<PIN>
    ) -> Self {
        Self {
            tx_transfer: None,
            raw_data: Some((stream, tx, Buffer::new(buffer))),
            dma_config,
            state_pin, 
        }
    }
}

pub  trait BufferWriter {
    fn add_str(&mut self, string: &str) -> Result<(), Error>;
    fn add(&mut self, data: &[u8]) -> Result<(), Error>;
    fn add_bool(&mut self, value: bool) -> Result<(), Error>;
    fn add_u8(&mut self, byte: u8) -> Result<(), Error>;
    fn add_u16(&mut self, value: u16) -> Result<(), Error>;
    fn add_u32(&mut self, value: u32) -> Result<(), Error>;
    fn add_u64(&mut self, value: u64) -> Result<(), Error>;
    fn add_f32(&mut self, value: f32) -> Result<(), Error>;
    fn add_f64(&mut self, value: f64) -> Result<(), Error>;
    fn clear(&mut self);
}

pub struct Buffer<const BUFFER_SIZE: usize> {
    buffer: &'static mut [u8; BUFFER_SIZE],
    size: usize,
}

impl <const BUFFER_SIZE: usize> Buffer<BUFFER_SIZE> {
    pub fn new(buffer: &'static mut [u8; BUFFER_SIZE]) -> Self {
        Self { buffer, size: 0 }
    }

    pub fn free(self) -> &'static mut [u8; BUFFER_SIZE] {
        self.buffer
    }
}

unsafe impl <const BUFFER_SIZE: usize> ReadBuffer for Buffer<BUFFER_SIZE> {
    type Word = u8;

    unsafe fn read_buffer(&self) -> (*const Self::Word, usize) {
        let ptr = self.buffer.as_ptr();
        (ptr, self.size)
    }
}
impl Write for dyn BufferWriter {
    #[inline(always)]
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        self.add_str(s).map_err(|_| core::fmt::Error)
    }
}

impl <const BUFFER_SIZE: usize> BufferWriter for Buffer<BUFFER_SIZE> {
    fn add_str(&mut self, string: &str) -> Result<(), Error> {
        self.add(string.as_bytes())
    }

    fn add(&mut self, data: &[u8]) -> Result<(), Error> {
        if self.size + data.len() > BUFFER_SIZE {
            return Err(Error::DmaBufferOverflow);
        }        
        let len = data.len();
        self.buffer[self.size..self.size + len].copy_from_slice(data);
        self.size += len;        
        Ok(())
    }

    fn add_bool(&mut self, value: bool) -> Result<(), Error> {
        if self.size + 1 > BUFFER_SIZE {
            return Err(Error::DmaBufferOverflow);
        }
        self.buffer[self.size] = 0;// if value { 1_u8 } else { 0_u8 };
        self.size += 1;
        Ok(())
    }

    fn add_u8(&mut self, byte: u8) -> Result<(), Error> {
        if self.size + 1 > BUFFER_SIZE {
            return Err(Error::DmaBufferOverflow);
        }
        self.buffer[self.size] = byte;
        self.size += 1;
        Ok(())
    }

    #[inline(always)]
    fn add_u16(&mut self, value: u16) -> Result<(), Error> {
        self.add(&value.to_be_bytes())
    }

    #[inline(always)]
    fn add_u32(&mut self, value: u32) -> Result<(), Error> {
        self.add(&value.to_be_bytes())
    }

    #[inline(always)]
    fn add_u64(&mut self, value: u64) -> Result<(), Error> {
        self.add(&value.to_be_bytes())
    }
    #[inline(always)]
    fn add_f32(&mut self, value: f32) -> Result<(), Error> {
        self.add(&value.to_be_bytes())
    }

    #[inline(always)]
    fn add_f64(&mut self, value: f64) -> Result<(), Error> {
        self.add(&value.to_be_bytes())
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.size = 0;
    }
}

#[derive(Clone, Copy)]
pub struct LedState(u8);

impl LedState {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn set_high(&mut self, num: u8, high: bool) {
        if high {
            self.0 = self.0 | (1 << num);
        } else {
            self.0 = self.0 & !(1 << num);
        }
    }
    
    pub fn toggle(&mut self, num: u8) {
        self.0 = self.0 ^ (1 << num);
    }
    
    pub fn set_mask(&mut self, mask: u8) {
        self.0 = mask;
    }

    pub fn is_high(&self, num: u8) -> bool {
        (self.0 & (1 << num)) != 0
    }
    
    pub fn get_pin_state(&self, num: u8) -> PinState {
        if self.is_high(num) {
            PinState::High
        } else {
            PinState::Low
        }
    }
}