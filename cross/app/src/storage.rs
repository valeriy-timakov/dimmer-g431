use embedded_hal::digital::v2::OutputPin;
use stm32g4xx_hal::flash::{FlashSize, Parts};

use crate::errors::Error;
use crate::pwm_service::PwmSettings;

pub struct Storage {
    flash: Parts,
}

const VALUE_PRESENT_SIGN: u64 = 0x12F8AC5926c1D2A3;
const STORAGE_START_ADDRESS: u32 = 0x18000;
const SIGN_SIZE: usize = size_of::<u64>();
const SIZE: usize = size_of::<PwmSettings>() + SIGN_SIZE;

impl Storage {
    pub fn new(flash: Parts) -> Self {


        unsafe {
            let mut flash = &(*stm32g4xx_hal::stm32::FLASH::ptr());
            flash.acr.modify(|_, w| {
                w.latency().bits(0b1000) // 8 wait states
            });
        }
        
        Storage {
            flash,
        }
    }

    pub fn erase(&mut self) -> Result<(), Error> {
        let mut flash_writer = self.flash.writer(FlashSize::Sz128K);
        flash_writer.change_verification(false);
        flash_writer.erase(STORAGE_START_ADDRESS, 128)
            .map_err(|err| {Error::FlashError(err)})
    }

    pub fn save(&mut self, settings: &PwmSettings) -> Result<(), Error> {
        let mut flash_writer = self.flash.writer(FlashSize::Sz128K);
        flash_writer.change_verification(false);
        
        flash_writer.erase(STORAGE_START_ADDRESS, 128)
            .map_err(|err| {Error::FlashError(err)})?;

        let bytes = unsafe {
            core::slice::from_raw_parts(
                (settings as *const PwmSettings) as *const u8,
                size_of::<PwmSettings>(),
            )
        };

        let mut buffer = [0_u8; SIZE];
        buffer[..SIGN_SIZE].copy_from_slice(&VALUE_PRESENT_SIGN.to_be_bytes());
        buffer[SIGN_SIZE..].copy_from_slice(bytes);
        
        let res = flash_writer.write(STORAGE_START_ADDRESS, &buffer, true)
            .map_err(|err| {
                Error::FlashError(err)
            });
        res
    }
    
    pub fn read(&mut self) -> Result<PwmSettings, Error> {
        let mut flash_writer = self.flash.writer(FlashSize::Sz128K);


        let bytes = flash_writer
            .read(STORAGE_START_ADDRESS, SIZE)
            .map_err(|err| {Error::FlashError(err)})?;
        
        if bytes.len() < SIZE {
            return Err(Error::StorageEmpty);
        }
        
        let sign_bytes = bytes[..SIGN_SIZE].try_into().unwrap();
        if u64::from_be_bytes(sign_bytes) != VALUE_PRESENT_SIGN {
            return Err(Error::StorageEmpty);
        }
        
        let result = PwmSettings::new(
            0, 0, 0, 0, 
            0, 0, 0, 0);
        
        let mut result_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                (&result as *const PwmSettings) as *mut u8, 
                size_of::<PwmSettings>(),    
            )
        };
        
        result_bytes.copy_from_slice(&bytes[SIGN_SIZE..]);
        
        Ok(result)
    }
    
    pub fn read_or_create(&mut self) -> Result<PwmSettings, Error> {
        match self.read() {
            Ok(settings) => Ok(settings),
            Err(Error::StorageEmpty) => {
                let result = PwmSettings::default();
                self.save(&result)?;
                Ok(result)
            },
            Err(err) => Err(err),
        }
    }
}