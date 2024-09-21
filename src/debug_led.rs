use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use embedded_hal::digital::v2::{OutputPin, PinState};

pub struct DebugLed<PIN> {
    pin: PIN,
    on_is_high: bool,
    busy: &'static AtomicBool,
    on: &'static AtomicBool,
    led_blink_half_period_ms: u32,
    pin_is_on: bool,
    last_pin_toggle_millis: u32,
}

impl <PIN> DebugLed<PIN>
    where
        PIN: OutputPin,
{
    pub fn new(pin: PIN, on_is_high: bool, busy: &'static AtomicBool, on: &'static AtomicBool, led_blink_half_period_ms: u32) -> Self {
        Self {
            pin,
            on_is_high,
            busy,
            on,
            led_blink_half_period_ms,
            pin_is_on: false,
            last_pin_toggle_millis: 0,
        }
    }
    
    pub fn is_on(&self) -> bool {
        self.on.load(Relaxed)
    }

    pub fn on(&mut self) {
        self.busy.store(true, Relaxed);
        let _ = self.pin.set_high();
    }

    pub fn off(&mut self) {
        let _ = self.pin.set_low();
        self.busy.store(false, Relaxed);
    }

    pub fn set(&mut self) {
        self.on.store(true, Relaxed);
    }

    pub fn clear(&mut self) {
        self.on.store(false, Relaxed);
    }

    pub fn tick(&mut self, curr_millis: u32) {
        if !self.busy.load(Relaxed) {
            let is_on = self.on.load(Relaxed);
            if is_on {
                if curr_millis - self.last_pin_toggle_millis > self.led_blink_half_period_ms {
                    self.pin_is_on = !self.pin_is_on;
                    let is_high = self.pin_is_on ^ self.on_is_high;
                    let _ = self.pin.set_state(if is_high { PinState::High } else { PinState::Low });
                    self.last_pin_toggle_millis = curr_millis;
                }
            }
        }
    }
}