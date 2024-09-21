use alloc::boxed::Box;
use fugit::RateExtU32;
use stm32g4xx_hal::gpio::gpioa::{PA0, PA1, PA10, PA11, PA12, PA15, PA2, PA3, PA4, PA6, PA7, PA8, PA9};
use stm32g4xx_hal::gpio::gpiob::{PB0, PB1, PB10, PB11, PB4, PB6, PB8, PB9};
use stm32g4xx_hal::hal::PwmPin;
use stm32g4xx_hal::pwm::PwmExt;
use stm32g4xx_hal::rcc::Rcc;
use stm32g4xx_hal::stm32::{TIM1, TIM15, TIM16, TIM17, TIM2, TIM3, TIM4, TIM8};
use stm32g4xx_hal::time::Hertz;

pub struct PwmSettings {
    pub group1_freq_hz: u32,
    pub group2_freq_hz: u32,
    pub group3_freq_hz: u32,
    pub group4_freq_hz: u32,
    pub group5_freq_hz: u32,
    pub group6_freq_hz: u32,
    pub group7_freq_hz: u32,
    pub group8_freq_hz: u32,
}

impl PwmSettings {
    
    const DEFAULT_FREQ: u32 = 1000;
    
    pub fn new(group1_freq: u32, group2_freq: u32, group3_freq: u32, group4_freq: u32,
               group5_freq: u32, group6_freq: u32, group7_freq: u32, group8_freq: u32
    ) -> Self {
        PwmSettings {
            group1_freq_hz: group1_freq,
            group2_freq_hz: group2_freq,
            group3_freq_hz: group3_freq,
            group4_freq_hz: group4_freq,
            group5_freq_hz: group5_freq,
            group6_freq_hz: group6_freq,
            group7_freq_hz: group7_freq,
            group8_freq_hz: group8_freq,
        }
    }
    
    pub fn default() -> Self {
        PwmSettings {
            group1_freq_hz: Self::DEFAULT_FREQ,
            group2_freq_hz: Self::DEFAULT_FREQ,
            group3_freq_hz: Self::DEFAULT_FREQ,
            group4_freq_hz: Self::DEFAULT_FREQ,
            group5_freq_hz: Self::DEFAULT_FREQ,
            group6_freq_hz: Self::DEFAULT_FREQ,
            group7_freq_hz: Self::DEFAULT_FREQ,
            group8_freq_hz: Self::DEFAULT_FREQ,
        }
    }
}

// 
// type PwmChComp<TIM, CHANNEL> = Pwm<TIM, CHANNEL, ComplementaryDisabled, ActiveHigh, ActiveHigh>;
// type PwmCh<TIM, CHANNEL> = Pwm<TIM, CHANNEL, ComplementaryImpossible, ActiveHigh, ActiveHigh>;
// 
const CHANNELS_16_COUNT: usize = 17;
const CHANNELS_32_COUNT: usize = 4;

pub struct PwmChannels {
    // ch1_1: PwmChComp<TIM1, C1>,
    // ch1_2: PwmChComp<TIM1, C2>,
    // ch1_3: PwmChComp<TIM1, C3>,
    // ch1_4: PwmChComp<TIM1, C4>,
    // ch2_1: PwmCh<TIM2, C1>,
    // ch2_2: PwmCh<TIM2, C2>,
    // ch2_3: PwmCh<TIM2, C3>,
    // ch2_4: PwmCh<TIM2, C4>,
    // ch3_1: PwmCh<TIM3, C1>,
    // ch3_2: PwmCh<TIM3, C2>,
    // ch3_3: PwmCh<TIM3, C3>,
    // ch3_4: PwmCh<TIM3, C4>,
    // ch4_1: PwmCh<TIM4, C1>,
    // ch4_2: PwmCh<TIM4, C2>,
    // ch4_3: PwmCh<TIM4, C3>,
    // ch4_4: PwmCh<TIM4, C4>,
    // ch5_1: PwmChComp<TIM8, C1>,
    // ch6_1: PwmChComp<TIM15, C1>,
    // ch6_2: PwmCh<TIM15, C2>,
    // ch7_1: PwmChComp<TIM16, C1>,
    // ch8_1: PwmChComp<TIM17, C1>,
    channels_32: [Box<dyn PwmPin<Duty=u32>>; CHANNELS_32_COUNT],
    channels_16: [Box<dyn PwmPin<Duty=u16>>; CHANNELS_16_COUNT],
}



impl PwmChannels {
    pub(crate) fn create<F>(tim1: TIM1, pins1: (PA8<F>, PA9<F>, PA10<F>, PA11<F>),
                            tim2: TIM2, pins2: (PA0<F>, PA1<F>, PB10<F>, PB11<F>),
                            tim3: TIM3, pins3: (PB4<F>, PA4<F>, PB0<F>, PB1<F>),
                            tim4: TIM4, pins4: (PB6<F>, PA12<F>, PB8<F>, PB9<F>),
                            tim8: TIM8, pin8: PA15<F>,
                            tim15: TIM15, pins15: (PA2<F>, PA3<F>),
                            tim16: TIM16, pin16: PA6<F>,
                            tim17: TIM17, pin17: PA7<F>,
                            settings: PwmSettings, rcc: &mut Rcc) -> Self {
        let group1 = tim2.pwm((pins2.0.into_alternate(), pins2.1.into_alternate(),
                               pins2.2.into_alternate(), pins2.3.into_alternate()), settings.group1_freq_hz.Hz(), rcc);
        let group2 = tim1.pwm((pins1.0.into_alternate(), pins1.1.into_alternate(),
                               pins1.2.into_alternate(), pins1.3.into_alternate()), settings.group2_freq_hz.Hz(), rcc);
        let group3 = tim3.pwm((pins3.0.into_alternate(), pins3.1.into_alternate(),
                               pins3.2.into_alternate(), pins3.3.into_alternate()), settings.group3_freq_hz.Hz(), rcc);
        let group4 = tim4.pwm((pins4.0.into_alternate(), pins4.1.into_alternate(),
                               pins4.2.into_alternate(), pins4.3.into_alternate()), settings.group4_freq_hz.Hz(), rcc);
        let group5 = tim8.pwm(pin8.into_alternate(), settings.group5_freq_hz.Hz(), rcc);
        let group6 = tim15.pwm((pins15.0.into_alternate(),
                                pins15.1.into_alternate()), settings.group6_freq_hz.Hz(), rcc);
        let group7 = tim16.pwm(pin16.into_alternate(), settings.group7_freq_hz.Hz(), rcc);
        let group8 = tim17.pwm(pin17.into_alternate(), settings.group8_freq_hz.Hz(), rcc);
        PwmChannels {
            channels_32: [
                Box::new(group1.0),
                Box::new(group1.1),
                Box::new(group1.2),
                Box::new(group1.3),
            ],
            channels_16: [
                Box::new(group2.0),
                Box::new(group2.1),
                Box::new(group2.2),
                Box::new(group2.3),
                Box::new(group3.0),
                Box::new(group3.1),
                Box::new(group3.2),
                Box::new(group3.3),
                Box::new(group4.0),
                Box::new(group4.1),
                Box::new(group4.2),
                Box::new(group4.3),
                Box::new(group5),
                Box::new(group6.0),
                Box::new(group6.1),
                Box::new(group7),
                Box::new(group8),
            ],
        }
    }

    fn set_enabled_channel(&mut self, channel: u8, enabled: bool) -> Result<(), crate::app::Error> {
        if channel < CHANNELS_32_COUNT as u8 {
            if enabled {
                Ok(self.channels_32[channel as usize].enable())
            } else {
                Ok(self.channels_32[channel as usize].disable())
            }
        } else if channel < (CHANNELS_16_COUNT + CHANNELS_32_COUNT) as u8 {
            if enabled {
                Ok(self.channels_16[channel as usize - CHANNELS_32_COUNT].enable())
            } else {
                Ok(self.channels_16[channel as usize - CHANNELS_32_COUNT].disable())
            }
        } else {
            Err(crate::app::Error::ChannelNotFound(channel))
        }
    }

    fn set_channel_duty(&mut self, channel_no: u8, duty: f32) -> Result<(), crate::app::Error> {
        if duty > 1.0 || duty < 0.0 {
            return Err(crate::app::Error::DutyOverflow(duty));
        }
        if channel_no < CHANNELS_32_COUNT as u8 {
            let channel = &mut self.channels_32[channel_no as usize];
            let duty = (duty * channel.get_max_duty() as f32) as u32;
            channel.set_duty(duty);
            channel.enable();
            Ok(())
        } else if channel_no < (CHANNELS_16_COUNT +CHANNELS_32_COUNT) as u8 {
            let channel =
                &mut self.channels_16[channel_no as usize - CHANNELS_32_COUNT];
            let duty = (duty * channel.get_max_duty() as f32) as u16;
            channel.set_duty(duty);
            channel.enable();
            Ok(())
        } else {
            Err(crate::app::Error::ChannelNotFound(channel_no))
        }
    }

    fn get_channel_duty(&mut self, channel_no: u8, duty: f32) -> Result<f32, crate::app::Error> {
        if duty > 1.0 || duty < 0.0 {
            return Err(crate::app::Error::DutyOverflow(duty));
        }
        if channel_no < CHANNELS_32_COUNT as u8 {
            let channel = &mut self.channels_32[channel_no as usize];
            let duty = channel.get_duty() as f32 / channel.get_max_duty() as f32;
            Ok(duty)
        } else if channel_no < (CHANNELS_16_COUNT + CHANNELS_32_COUNT) as u8 {
            let channel =
                &mut self.channels_16[channel_no as usize - CHANNELS_32_COUNT];
            let duty = channel.get_duty() as f32 / channel.get_max_duty() as f32;
            Ok(duty)
        } else {
            Err(crate::app::Error::ChannelNotFound(channel_no))
        }
    }

}