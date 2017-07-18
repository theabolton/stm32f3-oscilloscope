// stm32f3-buttons: buttons to LEDs

#![feature(core_intrinsics)]
#![feature(used)]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_rt;
extern crate stm32f30x;

use cortex_m::{asm, exception};
use cortex_m::peripheral::{SCB, SYST};
use stm32f30x::{GPIOE, RCC};

enum Led {
    LD4 = 8, LD3, LD5,
}

use Led::*;

const ON: bool = true;
const OFF: bool = false;

fn led(led: Led, state: bool) {
    let pin = led as u8;
    if pin >= 8 && pin <= 15 {
        let gpioe = GPIOE.get();
        match state {
            false => {
                unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << (pin + 16))); }
            }
            true => {
                unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << pin)); }
            }
        }
    }
}

#[inline(never)]
fn main() {
    cortex_m::interrupt::free(|cs| {
        // borrow peripherals
        let rcc = RCC.borrow(cs);
        let gpioe = GPIOE.borrow(cs);
        let syst = SYST.borrow(cs);
        let scb = SCB.borrow(cs);
        // power on GPIOE
        rcc.ahbenr.modify(|_, w| w.iopeen().enabled());
        // set pins 8 (LD4), 9 (LD3), and 10 (LD5) for output
        gpioe.moder.modify(|_, w| w.moder8().output()
                                   .moder9().output()
                                   .moder10().output());
        // enable Cortex-M SysTick counter
        syst.set_reload(8000); // set to update every 8000 clocks, or every 1ms
        unsafe { scb.shpr[11].write(0xf0); } // set SysTick exception (interrupt) priority to lowest possible
        syst.clear_current();
        // -FIX- SVD has incorrect identifiers here, so the API is nonsensical:
        unsafe { syst.csr.write(0b100); } // set clock to AHB, not AHB/8
        //iprintln!(&itm.stim[0], "SysTick source (should say 'Core' for AHB, 'External' for AHB/8): {:?}", syst.get_clock_source());
        syst.enable_interrupt();
        syst.enable_counter();
        // turn on LD4 (northwest, blue) to show we've gotten this far
        led(LD4, ON);
    });

    loop {
        led(LD5, ON); // northeast, orange
        delay_ms(500);
        led(LD5, OFF);
        delay_ms(500);
    }
}

#[allow(dead_code)]
#[used]
#[link_section = ".rodata.exceptions"]
static EXCEPTIONS: exception::Handlers = exception::Handlers {
    // override the default SysTick handler
    sys_tick: systick_handler,
    ..exception::DEFAULT_HANDLERS
};

static mut TIMING_DELAY: u32 = 0;

extern "C" fn systick_handler(_: exception::SysTick) {
    // turn on LD3 (north, red) to show we got a systick exception
    led(LD3, ON);
    // decrement delay
    unsafe {
        if TIMING_DELAY != 0 {
            TIMING_DELAY -= 1;
        }
    }
}

fn delay_ms(ms: u32) {
    unsafe {
        core::intrinsics::volatile_store(&mut TIMING_DELAY, ms);
        while core::intrinsics::volatile_load(&TIMING_DELAY) != 0 {}
    }
}

#[allow(dead_code)]
#[used]
#[link_section = ".rodata.interrupts"]
static INTERRUPTS: [extern "C" fn(); 240] = [default_handler; 240];

extern "C" fn default_handler() {
    asm::bkpt();
}
