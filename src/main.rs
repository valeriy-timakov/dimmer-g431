#![allow(unsafe_code)]
#![allow(warnings)]
#![allow(missing_docs)]
#![allow(unused_variables)]
#![no_main]
#![no_std]


extern crate alloc;

use core::panic::PanicInfo;
use core::sync::atomic;
use core::sync::atomic::Ordering;

mod pwm_service;

#[rtic::app(device = stm32g4xx_hal::stm32g4::stm32g431, peripherals = true)]
mod app {
    use cortex_m::delay::Delay;
    use embedded_alloc::Heap;
    use fugit::ExtU32;
    use stm32g4xx_hal::delay::DelayFromCountDownTimer;
    use stm32g4xx_hal::gpio::{Output, PushPull};
    use stm32g4xx_hal::gpio::gpioc::PC6;
    use stm32g4xx_hal::hal::PwmPin;
    use stm32g4xx_hal::prelude::*;
    use stm32g4xx_hal::pwm::{Pins, PwmExt};
    use stm32g4xx_hal::rcc::{RccExt};
    use stm32g4xx_hal::stm32::{TIM6};
    use stm32g4xx_hal::timer::{CountDownTimer, Timer};

    use crate::pwm_service::{PwmChannels, PwmSettings};

    const LOG_LEVEL: log::LevelFilter = log::LevelFilter::Info;



    #[global_allocator]
    static HEAP: Heap = Heap::empty();
    
    // Resources shared between tasks
    #[shared]
    struct Shared {}

    // Local resources to specific tasks (cannot be shared)
    #[local]
    struct Local {
        led: PC6<Output<PushPull>>,
        delay_syst: Delay,
        delay_tim2: DelayFromCountDownTimer<CountDownTimer<TIM6>>, 
    }
    
    
    pub enum Error {
        ChannelNotFound(u8),
        DutyOverflow(f32),
    }


    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {

        let dp = cx.device;
        let cp = cx.core;

        let rcc = dp.RCC.constrain();        
        
        let mut pll_config = stm32g4xx_hal::rcc::PllConfig::default();
        // Sysclock is based on PLL_R
        pll_config.mux = stm32g4xx_hal::rcc::PLLSrc::HSE(8.mhz()); // 8MHz
        pll_config.n = stm32g4xx_hal::rcc::PllNMul::MUL_32;
        pll_config.m = stm32g4xx_hal::rcc::PllMDiv::DIV_1; // f(vco) = 8MHz*32/1 = 256MHz
        pll_config.r = Some(stm32g4xx_hal::rcc::PllRDiv::DIV_2); // f(sysclock) = 256MHz/2 = 128MHz

        // Note to future self: The AHB clock runs the timers, among other things.
        // Please refer to the Clock Tree manual to determine if it is worth
        // changing to a lower speed for battery life savings.
        let clock_config = stm32g4xx_hal::rcc::Config::default()
            .pll_cfg(pll_config)
            .clock_src(stm32g4xx_hal::rcc::SysClockSrc::PLL);

        // After clock configuration, the following should be true:
        // Sysclock is 128MHz
        // AHB clock is 128MHz
        // APB1 clock is 128MHz
        // APB2 clock is 128MHz
        // The ADC will ultimately be put into synchronous mode and will derive
        // its clock from the AHB bus clock, with a prescalar of 2 or 4.

        //let pwr = dp.PWR.constrain().freeze();
        let mut rcc = rcc.freeze(clock_config);




        let gpioa = dp.GPIOA.split(&mut rcc);
        let gpiob = dp.GPIOB.split(&mut rcc);
        let gpioc = dp.GPIOC.split(&mut rcc);
        
        
        let pwm_settings = PwmSettings::new(
            1.khz(),
            5.khz(),
            1900.hz(),
            2500.hz(),
            20100.hz(),
            50200.hz(),
            200300.hz(),
            20.hz(),
            20.hz(),
        );
        
        let channels = PwmChannels::create(dp.TIM1, (gpioa.pa8, gpioa.pa9, gpioa.pa10, gpioa.pa11),
                                           dp.TIM2, (gpioa.pa0, gpioa.pa1, gpiob.pb10, gpiob.pb11),
                                           dp.TIM3, (gpiob.pb4, gpioa.pa4, gpiob.pb0, gpiob.pb1),
                                           dp.TIM4, (gpiob.pb6, gpioa.pa12, gpiob.pb8, gpiob.pb9),
                                           dp.TIM8, gpioa.pa15,
                                           dp.TIM15, (gpioa.pa2, gpioa.pa3),
                                           dp.TIM16, gpioa.pa6,
                                           dp.TIM17, gpioa.pa7,
                                           pwm_settings, &mut rcc);
        
        

/*

        let tx = gpioc.pc4.into_alternate();
        let rx = gpiob.pb7.into_alternate();
        let mut usart = dp
            .USART1
            .usart(tx, rx, FullConfig::default(), &mut rcc)
            .unwrap();



        let tx = gpioc.pc10.into_alternate();
        let rx = gpioc.pc11.into_alternate();
        let mut usart = dp
            .UART4
            .usart(tx, rx, FullConfig::default(), &mut rcc)
            .unwrap();


        let sclk = gpiob.pb13.into_alternate();
        let miso = gpiob.pb14.into_alternate();
        let mosi = gpiob.pb15.into_alternate();

        let spi = dp
            .SPI2
            .spi((sclk, miso, mosi), spi::MODE_0, 400.khz(), &mut rcc);
*/


        let mut led: PC6<Output<PushPull>> = gpioc.pc6.into_push_pull_output();

        let mut delay_syst = cp.SYST.delay(&rcc.clocks);

        let timer2 = Timer::new(dp.TIM6, &rcc.clocks);
        let mut delay_tim2: DelayFromCountDownTimer<CountDownTimer<TIM6>> = DelayFromCountDownTimer::new(timer2.start_count_down(10.hz()));


        (
            // Initialization of shared resources
            Shared {},
            // Initialization of task local resources
            Local {
                led,
                delay_syst,
                delay_tim2, 
            },
            // Move the monotonic timer to the RTIC run-time, this enables
            // scheduling
            init::Monotonics(),
        )
    }

    // Background task, runs whenever no other tasks are running
    #[idle (local = [led, delay_syst, delay_tim2])]
    fn idle(mut cx: idle::Context) -> ! {
        loop {
            cx.local.led.toggle().unwrap();
            cx.local.delay_syst.delay_ms(1000);
            cx.local.led.toggle().unwrap();
            cx.local.delay_tim2.delay_ms(3000_u16);
            // Sleep until next interrupt
            //cortex_m::asm::wfi();
        }
    }

}



#[cfg(feature = "defmt")]
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}


#[cfg(feature = "defmt")]
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

#[inline(never)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        atomic::compiler_fence(Ordering::SeqCst);
    }
}