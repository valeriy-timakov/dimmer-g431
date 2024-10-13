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
mod counter;
mod communication;
mod storage;
mod debug_led;
mod server;
mod errors;

fn compare_arrays(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

#[rtic::app(device = stm32g4xx_hal::stm32g4::stm32g431, peripherals = true)]
mod app {
    use core::fmt::Write;
    use core::sync::atomic::AtomicBool;
    use core::sync::atomic::Ordering::Relaxed;
    use cortex_m::delay::Delay;

    use embedded_alloc::Heap;
    use embedded_dma::{ReadBuffer, StaticReadBuffer};
    use embedded_hal::digital::v2::OutputPin;
    use fugit::{ExtU32, RateExtU32};
    use rtic::Mutex;
    use stm32g4xx_hal::delay::{CountDown};
    use stm32g4xx_hal::dma::{  TransferExt};
    use stm32g4xx_hal::dma::config::DmaConfig;
    use stm32g4xx_hal::dma::stream::{DMAExt, Stream0, Stream1};
    use stm32g4xx_hal::dma::transfer::{CircTransfer};
    use stm32g4xx_hal::flash::{FlashExt};
    use stm32g4xx_hal::flash::Error::{AddressLargerThanFlash, AddressMisaligned, ArrayMustBeDivisibleBy8, EraseError, LengthNotMultiple2, LengthTooLong, LockError, OptLockError, OptUnlockError, ProgrammingError, UnlockError, VerifyError, WriteError};
    use stm32g4xx_hal::gpio::{Alternate, ExtiPin, Input, Output, PullDown, PushPull, SignalEdge};
    use stm32g4xx_hal::gpio::gpioa::*;
    use stm32g4xx_hal::gpio::gpiob::*;
    use stm32g4xx_hal::gpio::gpioc::*;
    use stm32g4xx_hal::hal::PwmPin;
    use stm32g4xx_hal::prelude::*;
    use stm32g4xx_hal::pwm::{Pins, PwmExt};
    use stm32g4xx_hal::pwr::PwrExt;
    use stm32g4xx_hal::rcc::RccExt;
    use stm32g4xx_hal::serial::{DMA, FullConfig, Rx, Tx};
    use stm32g4xx_hal::stm32::{DMA1, TIM6, TIM7, USART1};
    use stm32g4xx_hal::syscfg::SysCfgExt;
    use stm32g4xx_hal::timer::{CountDownTimer, Event, Timer};

    use crate::communication::{Buffer, LedState, TxTransfer};
    use crate::debug_led::DebugLed;
    use crate::pwm_service::{PwmChannels, PwmSettings};
    use crate::storage::Storage;
    use postcard::{to_slice_crc32};
    use crc::{Crc, CRC_32_ISCSI};
    use crate::errors::Error;
    use crate::get_device_id;
    use crate::server::Server;

    const LOG_LEVEL: log::LevelFilter = log::LevelFilter::Info;


    const FLASH_EXAMPLE_START_ADDRESS: u32 = 0x8000;
    const BUFFER_SIZE: usize = 256;
    const LED_ERROR_BLINK_PERIOD: u32 = 100;

    static TX_ERROR: AtomicBool = AtomicBool::new(false);
    static BUSY: AtomicBool = AtomicBool::new(false);

    #[global_allocator]
    static HEAP: Heap = Heap::empty();

    static ERROR: AtomicBool = AtomicBool::new(false);

    
    // Resources shared between tasks
    #[shared]
    struct Shared {
        rx_transfer1: CircTransfer<Stream0<DMA1>, Rx<USART1, PB7<Alternate<7>>, DMA>, &'static mut [u8]>,
        tx_transfer1: TxTransfer<Stream1<DMA1>, Tx<USART1, PC4<Alternate<7>>, DMA>, Buffer<BUFFER_SIZE>, PA5<Output<PushPull>>>,
        delay: Delay,
        leds_state: LedState,
        led0: PB2<Output<PushPull>>,
        led1: PB3<Output<PushPull>>,
        millis_from_start: u64,
        server: Server,
        storage: Storage,
    }

    // Local resources to specific tasks (cannot be shared)
    #[local]
    struct Local {
        led: PC6<Output<PushPull>>,
        led2: PB5<Output<PushPull>>,
        led4: PB12<Output<PushPull>>,
        button: PC13<Input<PullDown>>,
        timer7: CountDownTimer<TIM7>,
        timer6: CountDownTimer<TIM6>,
        channels: PwmChannels,
    }


    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {

        let mut dp = cx.device;
        let cp = cx.core;

        let rcc = dp.RCC.constrain();        
        
        let mut pll_config = stm32g4xx_hal::rcc::PllConfig::default();
        // Sysclock is based on PLL_R
        pll_config.mux = stm32g4xx_hal::rcc::PllSrc::HSE(8.MHz()); // 8MHz
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

        let pwr = dp.PWR.constrain().freeze();
        let mut rcc = rcc.freeze(clock_config, pwr);
        let mut syscfg = dp.SYSCFG.constrain();

        let mut timer7 = Timer::new(dp.TIM7, &rcc.clocks);
        let mut timer7 = timer7.start_count_down(20.millis());
        timer7.listen(Event::TimeOut);


        let timer6 = Timer::new(dp.TIM6, &rcc.clocks);
        let mut timer6 = timer6.start_count_down(1.millis());
        timer6.listen(Event::TimeOut);
        
        let gpioa = dp.GPIOA.split(&mut rcc);
        let gpiob = dp.GPIOB.split(&mut rcc);
        let gpioc = dp.GPIOC.split(&mut rcc);


        let mut led = gpioc.pc6.into_push_pull_output();
        let mut led0 = gpiob.pb2.into_push_pull_output();
        let mut led1 = gpiob.pb3.into_push_pull_output();
        let mut led2 = gpiob.pb5.into_push_pull_output();
        let mut led3 = gpioa.pa5.into_push_pull_output();
        let mut led4 = gpiob.pb12.into_push_pull_output();

        let mut storage: Storage = Storage::new(dp.FLASH.constrain());
        let pwm_settings = match storage.read_or_create_pwm_settings() {
            Ok(settings) => settings,
            Err(Error::FlashError(_)) => {
                PwmSettings::default()
            },
            Err(_) => {
                PwmSettings::default()
            },
        };
        
        let mut channels = PwmChannels::create(
                               dp.TIM2, (gpioa.pa0, gpioa.pa1, gpiob.pb10, gpiob.pb11),
                               dp.TIM1, (gpioa.pa8, gpioa.pa9, gpioa.pa10, gpioa.pa11),
                               dp.TIM3, (gpiob.pb4, gpioa.pa4, gpiob.pb0, gpiob.pb1),
                               dp.TIM4, (gpiob.pb6, gpioa.pa12, gpiob.pb8, gpiob.pb9),
                               dp.TIM8, gpioa.pa15,
                               dp.TIM15, (gpioa.pa2, gpioa.pa3),
                               dp.TIM16, gpioa.pa6,
                               dp.TIM17, gpioa.pa7,
                               pwm_settings, &mut rcc);
        channels.set_enabled_all_same(false);
        channels.set_channel_duty(1, 0.7).unwrap();
        
        led2.set_high().unwrap();

        let tx = gpioc.pc4.into_alternate();
        let rx = gpiob.pb7.into_alternate();
        let mut usart = dp
            .USART1
            .usart(tx, rx, FullConfig::default()
                .baudrate(115200.bps())
                .receiver_timeout_us(1000), &mut rcc)
            .unwrap();

        let rx_buffer = cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap();
        let tx_buffer = cortex_m::singleton!(: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE]).unwrap();

        let (tx, mut rx) = usart.split();
        
        rx.listen();

        let streams = dp.DMA1.split(&rcc);
        let rx_config = DmaConfig::default()
            .transfer_complete_interrupt(false)
            .transfer_error_interrupt(true)
            .circular_buffer(true)
            .memory_increment(true);


        led.set_high().unwrap();
        
        let mut rx_transfer1: CircTransfer<Stream0<DMA1>, Rx<USART1, PB7<Alternate<7>>, DMA>, &mut [u8]> = 
            streams.0.into_circ_peripheral_to_memory_transfer(
            rx.enable_dma(),
            &mut rx_buffer[..],
            rx_config,
        );

        let tx_config: DmaConfig = DmaConfig::default()
            .transfer_complete_interrupt(true)
            .transfer_error_interrupt(true)
            .circular_buffer(false)
            .memory_increment(true);
        let tx_debug_led = 
            DebugLed::new(led3, true, &BUSY, &TX_ERROR, LED_ERROR_BLINK_PERIOD);
        let mut tx_transfer1 = 
            TxTransfer::new_sb(streams.1, tx.enable_dma(), tx_buffer, tx_config, tx_debug_led);
        

        rx_transfer1.start(|_rx| {});

/*

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

        let mut button = gpioc.pc13.into_pull_down_input();
        button.make_interrupt_source(&mut syscfg);
        button.trigger_on_edge(&mut dp.EXTI, SignalEdge::RisingFalling);
        button.enable_interrupt(&mut dp.EXTI);



        use stm32g4xx_hal::time::ExtU32;

        let mut delay = cp.SYST.delay(&rcc.clocks);
        
        let device_settings = match storage.read_device_settings() {
            Ok(settings) => Some(settings),
            Err(_) => None,
        };


        (
            // Initialization of shared resources
            Shared {
                rx_transfer1, 
                tx_transfer1,
                delay,
                leds_state: LedState::new(),
                led0,
                led1,
                millis_from_start: 0,
                server: Server::new(get_device_id(), device_settings.map(|s| s.device_no)),
                storage,
            },
            // Initialization of task local resources
            Local {
                led2,
                led,
                led4,
                button,
                timer6, 
                timer7,
                channels,
            },
            // Move the monotonic timer to the RTIC run-time, this enables
            // scheduling
            init::Monotonics(),
        )
    }

    // Background task, runs whenever no other tasks are running
    #[idle (local = [])]
    fn idle(cx: idle::Context) -> ! {
        loop {
            // Sleep until next interrupt
            cortex_m::asm::wfi();
        }
    }


    #[task(binds = EXTI15_10, priority=2, local = [button, led4], shared=[tx_transfer1, delay, leds_state, storage])]
    fn button_pressed(mut ctx: button_pressed::Context) {
        let btn = ctx.local.button;
        btn.clear_interrupt_pending_bit();

        // let eight_bytes = [0x78, 0x67, 0x15, 0x92, 0xac, 0xbe, 0x4d, 0x1fu8];
        // let mut flash_writer = ctx.local.flash.writer(FlashSize::Sz128K);
        ctx.shared.delay.lock(|delay| {
            delay.delay_ms(5_u32);
        });
        let down = btn.is_high().unwrap();
        if down {
            ctx.shared.tx_transfer1.lock(|tx_transfer| {
                tx_transfer.clear_error();
                tx_transfer.send_silent(|buf| {
                    buf.add_str("pressed\n")
                });
            });
            ctx.shared.delay.lock(|delay| {
                delay.delay_ms(500_u32);
            });
            
            let mut d = match ctx.shared.storage.lock(|s| {s.read_pwm_settings()}) {
                Ok(settings) => {
                    ctx.shared.tx_transfer1.lock(|tx_transfer| {
                        tx_transfer.send_silent(|buf| {
                            buf.add_str("readed\n")
                        });
                    });
                    settings
                },
                Err(_) => {
                    ctx.shared.tx_transfer1.lock(|tx_transfer| {
                        tx_transfer.send_silent(|buf| {
                            buf.add_str("read error!\n")
                        });
                    });
                    PwmSettings::default()
                },
            };
            ctx.shared.delay.lock(|delay| {
                delay.delay_ms(300_u32);
            });
            ctx.shared.tx_transfer1.lock(|tx_transfer| {
                tx_transfer.send_silent(|buf| {
                    buf.add_str("before save Frreq=(\n")?;
                    let mut buffer = itoa::Buffer::new();
                    buf.add_str("1: ")?;
                    buf.add_str(buffer.format(d.group1_freq_hz));
                    buf.add_str("\n2: ")?;
                    buf.add_str(buffer.format(d.group2_freq_hz));
                    buf.add_str("\n3: ")?;
                    buf.add_str(buffer.format(d.group3_freq_hz));
                    buf.add_str("\n4: ")?;
                    buf.add_str(buffer.format(d.group4_freq_hz));
                    buf.add_str("\n5: ")?;
                    buf.add_str(buffer.format(d.group5_freq_hz));
                    buf.add_str("\n6: ")?;
                    buf.add_str(buffer.format(d.group6_freq_hz));
                    buf.add_str("\n7: ")?;
                    buf.add_str(buffer.format(d.group7_freq_hz));
                    buf.add_str("\n8: ")?;
                    buf.add_str(buffer.format(d.group8_freq_hz));
                    buf.add_str("\n);\n")
                });
            });
            ctx.shared.delay.lock(|delay| {
                delay.delay_ms(300_u32);
            });
            d.group1_freq_hz = d.group1_freq_hz + 50;
            d.group2_freq_hz = d.group2_freq_hz + 100;
            d.group3_freq_hz = d.group3_freq_hz + 10;
            d.group4_freq_hz = d.group4_freq_hz - 1;
            d.group5_freq_hz = d.group2_freq_hz + 200;
            d.group6_freq_hz = d.group2_freq_hz + 1000;
            d.group7_freq_hz = d.group2_freq_hz + 800;
            d.group8_freq_hz = d.group2_freq_hz + 900;


            ctx.shared.tx_transfer1.lock(|tx_transfer| {
                tx_transfer.send_silent(|buf| {
                    buf.add_str("Saving: ")
                });
            });
            ctx.shared.delay.lock(|delay| {
                delay.delay_ms(300_u32);
            });
            let res_str = match ctx.shared.storage.lock(|s|{s.save_pwm_settings(&d)}) {
                Ok(_) => {
                    "Ok!\n"
                }
                Err(Error::FlashError(er)) => {
                    map_flash_error(er)
                }
                Err(_) => {
                    "undefined error!"
                }
            };
            ctx.shared.tx_transfer1.lock(|tx_transfer| {
                tx_transfer.send_silent(|buf| {
                    buf.add_str(res_str)?;
                    buf.add_str("\n")
                });
            });
            ctx.shared.delay.lock(|delay| {
                delay.delay_ms(300_u32);
            });
            
            
            
            // match flash_writer.erase(FLASH_EXAMPLE_START_ADDRESS, 128) {
            //     Ok(_) => {
            //         ctx.shared.leds_state.lock(|leds_state| {
            //             leds_state.set_high(0, true);
            //         });
            //     }
            //     Err(stm32g4xx_hal::flash::Error::LengthTooLong) => {
            //         // ctx.shared.leds_state.lock(|leds_state| {
            //         //     leds_state.set_high(1, true);
            //         // });
            //     }
            //     Err(stm32g4xx_hal::flash::Error::LengthNotMultiple2) => {
            //         // ctx.shared.leds_state.lock(|leds_state| {
            //         //     leds_state.set_high(2, true);
            //         // });
            //     }
            // 
            //     Err(stm32g4xx_hal::flash::Error::AddressLargerThanFlash) => {
            //         // ctx.shared.leds_state.lock(|leds_state| {
            //         //     leds_state.set_high(3, true);
            //         // });
            //     }
            // 
            //     Err(_) => {
            //         // ctx.shared.leds_state.lock(|leds_state| {
            //         //     leds_state.set_high(0, true);
            //         //     leds_state.set_high(1, true);
            //         // });
            //     }
            // }
            // 
            // match flash_writer.write(FLASH_EXAMPLE_START_ADDRESS, &eight_bytes, true) {
            //     Ok(_) => {
            //         ctx.shared.leds_state.lock(|leds_state| {
            //             leds_state.set_high(1, true);
            //         });
            //        
            //     }
            //     Err(_) => {
            //         // ctx.shared.leds_state.lock(|leds_state| {
            //         //     leds_state.set_high(2, true);
            //         // });
            //     }
            // }
        } else {
            // let bytes = flash_writer
            //     .read(FLASH_EXAMPLE_START_ADDRESS, eight_bytes.len())
            //     .unwrap();
            // if compare_arrays(&eight_bytes, &bytes) {
            //     ctx.shared.leds_state.lock(|leds_state| {
            //         leds_state.set_high(2, true);
            //     });
            // } else {
            //     ctx.shared.leds_state.lock(|leds_state| {
            //         leds_state.set_high(3, true);
            //     });
            // }
            
        }
    }


    fn map_flash_error(er: stm32g4xx_hal::flash::Error) -> &'static str {
        match er {
            AddressLargerThanFlash => "AddressLargerThanFlash",
            AddressMisaligned => "AddressMisaligned",
            LengthNotMultiple2 => "LengthNotMultiple2",
            LengthTooLong => "LengthTooLong",
            EraseError => "EraseError",
            ProgrammingError => "ProgrammingError",
            WriteError => "WriteError",
            VerifyError => "VerifyError",
            UnlockError => "UnlockError",
            OptUnlockError => "OptUnlockError",
            LockError => "LockError",
            OptLockError => "OptLockError",
            ArrayMustBeDivisibleBy8 => "ArrayMustBeDivisibleBy8",
        }
    }


    #[task(binds = DMA1_CH1, priority=3, shared = [rx_transfer1])]
    fn dma1_ch1(mut ctx: dma1_ch1::Context) {
        ctx.shared.rx_transfer1.lock(|rx_transfer: &mut CircTransfer<Stream0<DMA1>, Rx<USART1, PB7<Alternate<7>>, DMA>, &'static mut [u8]>| {
            rx_transfer.clear_interrupts();
        });
    }

    #[task(binds = DMA1_CH2, priority=5, shared = [tx_transfer1])]
    fn dma1_ch2(mut ctx: dma1_ch2::Context) {
        let _ = ctx.shared.tx_transfer1.lock(|tx_transfer| {
            tx_transfer.on_transfer_complete().inspect_err(|_| {
                ERROR.store(true, Relaxed);
            })
        });
    }

    #[task(binds = TIM7, priority=2, local = [timer7, channels, led, led2], shared=[rx_transfer1, tx_transfer1, server, storage, led0, led1, leds_state, delay, millis_from_start])]
    fn tim7_irq(mut ctx: tim7_irq::Context) {
        ctx.local.timer7.clear_interrupt(Event::TimeOut);
        ctx.local.led.toggle().unwrap();
        let millis_from_start: u64 = ctx.shared.millis_from_start.lock(|millis_from_start| {
            *millis_from_start
        });
        ctx.shared.rx_transfer1.lock(|rx_transfer: &mut CircTransfer<Stream0<DMA1>, Rx<USART1, PB7<Alternate<7>>, DMA>, &'static mut [u8]>| {
            if rx_transfer.timeout_lapsed() {
                rx_transfer.clear_timeout();

                let mut data = [0; 1024];

                let data = rx_transfer.read_available(&mut data);
                if data.is_empty() {
                    return;
                }
                ctx.shared.tx_transfer1.lock(|tx_transfer| {
                    tx_transfer.send_silent(|buf| {
                        ctx.shared.server.lock(|server| {
                            ctx.shared.storage.lock(|storage| {
                                ctx.shared.leds_state.lock(|leds_state| {
                                    ctx.local.led2.set_high().unwrap();
                                    let res = server.idle(data, buf, storage, ctx.local.channels, leds_state);
                                    ctx.local.led2.set_low().unwrap();
                                    res
                                })
                            })
                        })
                    });
                    tx_transfer.tick(millis_from_start);
                });
                ctx.shared.delay.lock(|delay| {
                    delay.delay_ms(300_u32);
                });
            }
        });
        let leds_state_tmp = ctx.shared.leds_state.lock(|leds_state| {
            leds_state.clone()
        });
        ctx.shared.led0.lock(|led| {
            led.set_state(leds_state_tmp.get_pin_state(0)).unwrap();
        });
        ctx.shared.led1.lock(|led| {
            led.set_state(leds_state_tmp.get_pin_state(1)).unwrap();
        });
        ctx.local.led2.set_state(if ERROR.load(Relaxed) { PinState::High } else { PinState::Low }).unwrap();
    }
    
    #[task(binds = TIM6_DACUNDER, priority=2, local = [timer6], shared=[millis_from_start])]
    fn tim6_irq(mut ctx: tim6_irq::Context) {
        ctx.shared.millis_from_start.lock(|millis_from_start| {
            *millis_from_start += 1;
        });
    }

}


//
// #[cfg(feature = "defmt")]
// #[defmt::panic_handler]
// fn panic() -> ! {
//     cortex_m::asm::udf()
// }

#[inline(never)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        atomic::compiler_fence(Ordering::SeqCst);
    }
}


fn get_device_id() -> [u32;3] {
    unsafe {
        let ptr =  0x1FFF_7590 as *const u32;
        [ptr.read_volatile(), (ptr.offset(1)).read_volatile(), (ptr.offset(2)).read_volatile()]
    }
}

