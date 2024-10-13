use embedded_hal::digital::v2::OutputPin;
use stm32g4xx_hal::flash::{FlashSize, Parts};

use crate::errors::Error;
use crate::pwm_service::PwmSettings;

pub struct Storage {
    flash: Parts,
}

pub struct DeviceSettings {
    pub device_no: u8,
    //not used - to be proportional to 8 bytes
    pub _unused1: u8,
    pub _unused2: u16, 
    pub _unused3: u32,
}

impl DeviceSettings {
    pub fn new(device_no: u8) -> Self {
        DeviceSettings {
            device_no,
            _unused1: 0,
            _unused2: 0,
            _unused3: 0,
        }
    }
}

const VALUE_PRESENT_SIGN: u64 = 0x12F8AC5926c1D2A3;
const PWM_SETTINGS_START_ADDRESS: u32 = 0x18000;
const SIGN_SIZE: usize = size_of::<u64>();
const PWM_SETTINGS_SIZE: usize = size_of::<PwmSettings>() + SIGN_SIZE;
const DEVICE_SETTINGS_START_ADDRESS: u32 = PWM_SETTINGS_START_ADDRESS + PWM_SETTINGS_SIZE as u32;
const DEVICE_SETTINGS_SIZE: usize = size_of::<DeviceSettings>() + SIGN_SIZE;

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

    pub fn erase(&mut self, address: u32, size: usize) -> Result<(), Error> {
        let mut flash_writer = self.flash.writer(FlashSize::Sz128K);
        flash_writer.change_verification(false);
        flash_writer.erase(address, size)
            .map_err(|err| {Error::FlashError(err)})
    }

    pub fn save_pwm_settings(&mut self, settings: &PwmSettings) -> Result<(), Error> {
        self.save_settings::<PwmSettings, PWM_SETTINGS_SIZE>(settings, PWM_SETTINGS_START_ADDRESS)        
    }
    
    pub fn read_pwm_settings(&mut self) -> Result<PwmSettings, Error> {
        let mut result = PwmSettings::new(
            0, 0, 0, 0,
            0, 0, 0, 0);

        self.read_settings::<PwmSettings, PWM_SETTINGS_SIZE>(PWM_SETTINGS_START_ADDRESS, &mut result)?;
        
        Ok(result)
        
    }
    
    pub fn read_or_create_pwm_settings(&mut self) -> Result<PwmSettings, Error> {
        match self.read_pwm_settings() {
            Ok(settings) => Ok(settings),
            Err(Error::StorageEmpty) => {
                let result = PwmSettings::default();
                self.save_pwm_settings(&result)?;
                Ok(result)
            },
            Err(err) => Err(err),
        }
    }
    
    pub fn save_device_settings(&mut self, settings: &DeviceSettings) -> Result<(), Error> {
        self.save_settings::<DeviceSettings, DEVICE_SETTINGS_SIZE>(settings, DEVICE_SETTINGS_START_ADDRESS)        
    }
    
    pub fn read_device_settings(&mut self) -> Result<DeviceSettings, Error> {
        let mut result = DeviceSettings::new(0);

        self.read_settings::<DeviceSettings, DEVICE_SETTINGS_SIZE>(DEVICE_SETTINGS_START_ADDRESS, &mut result)?;
        
        Ok(result)
        
    }

    fn read_settings<T, const SIZE: usize>(&mut self, address: u32, data_to_fill: &mut T) -> Result<(), Error> {
        let mut flash_writer = self.flash.writer(FlashSize::Sz128K);

        let bytes = flash_writer
            .read(address, SIZE)
            .map_err(|err| {Error::FlashError(err)})?;

        if bytes.len() < SIZE {
            return Err(Error::StorageEmpty);
        }

        let sign_bytes = bytes[..SIGN_SIZE].try_into().unwrap();
        if u64::from_be_bytes(sign_bytes) != VALUE_PRESENT_SIGN {
            return Err(Error::StorageEmpty);
        }

        let mut result_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                (data_to_fill as *const T) as *mut u8,
                size_of::<T>(),
            )
        };

        result_bytes.copy_from_slice(&bytes[SIGN_SIZE..]);

        Ok(())
    }

    fn save_settings<T, const SIZE: usize>(&mut self, settings: &T, address: u32) -> Result<(), Error> {
        let mut flash_writer = self.flash.writer(FlashSize::Sz128K);
        flash_writer.change_verification(false);

        flash_writer.erase(address, SIZE)
            .map_err(|err| {Error::FlashError(err)})?;

        let bytes = unsafe {
            core::slice::from_raw_parts(
                (settings as *const T) as *const u8,
                size_of::<T>(),
            )
        };

        let mut buffer = [0_u8; SIZE];
        buffer[..SIGN_SIZE].copy_from_slice(&VALUE_PRESENT_SIGN.to_be_bytes());
        buffer[SIGN_SIZE..].copy_from_slice(bytes);

        let res = flash_writer.write(address, &buffer, true)
            .map_err(|err| {
                Error::FlashError(err)
            });
        res
    }

    // pub fn read_pwm_settings(&mut self) -> Result<PwmSettings, Error> {
    //     let mut flash_writer = self.flash.writer(FlashSize::Sz128K);
    // 
    // 
    //     let bytes = flash_writer
    //         .read(PWM_SETTINGS_START_ADDRESS, PWM_SETTINGS_SIZE)
    //         .map_err(|err| {Error::FlashError(err)})?;
    //     
    //     if bytes.len() < PWM_SETTINGS_SIZE {
    //         return Err(Error::StorageEmpty);
    //     }
    //     
    //     let sign_bytes = bytes[..SIGN_SIZE].try_into().unwrap();
    //     if u64::from_be_bytes(sign_bytes) != VALUE_PRESENT_SIGN {
    //         return Err(Error::StorageEmpty);
    //     }
    //     
    //     let result = PwmSettings::new(
    //         0, 0, 0, 0, 
    //         0, 0, 0, 0);
    //     
    //     let mut result_bytes = unsafe {
    //         core::slice::from_raw_parts_mut(
    //             (&result as *const PwmSettings) as *mut u8, 
    //             size_of::<PwmSettings>(),    
    //         )
    //     };
    //     
    //     result_bytes.copy_from_slice(&bytes[SIGN_SIZE..]);
    //     
    //     Ok(result)
    // }

    // pub fn save_pwm_settings(&mut self, settings: &PwmSettings) -> Result<(), Error> {
    //     let mut flash_writer = self.flash.writer(FlashSize::Sz128K);
    //     flash_writer.change_verification(false);
    //     
    //     flash_writer.erase(PWM_SETTINGS_START_ADDRESS, PWM_SETTINGS_SIZE)
    //         .map_err(|err| {Error::FlashError(err)})?;
    // 
    //     let bytes = unsafe {
    //         core::slice::from_raw_parts(
    //             (settings as *const PwmSettings) as *const u8,
    //             size_of::<PwmSettings>(),
    //         )
    //     };
    // 
    //     let mut buffer = [0_u8; PWM_SETTINGS_SIZE];
    //     buffer[..SIGN_SIZE].copy_from_slice(&VALUE_PRESENT_SIGN.to_be_bytes());
    //     buffer[SIGN_SIZE..].copy_from_slice(bytes);
    //     
    //     let res = flash_writer.write(PWM_SETTINGS_START_ADDRESS, &buffer, true)
    //         .map_err(|err| {
    //             Error::FlashError(err)
    //         });
    //     res
    // }
}