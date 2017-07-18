// stm32f3-buttons: buttons to LEDs

// STM32F3 to ST7735 display breakout board pushbuttons:
// J1-16       GND          black   GND
// J1-17   Button4 (right)  brown   PD15
// J1-18   Button3          white   PD14
// J1-19   Button2          grey    PD13
// J1-20   Button1 (left)   purple  PD12

// LEDs:
// LD4 (NW, blue)   - initialization completed
// LD3 (N, red)     - heartbeat (main loop)
// LD8 (SW, orange) - Button1 (left)
// LD10 (S, red)    - Button2
// LD9 (SE, blue)   - Button3
// LD7 (E, green)   - Button4 (right)

#![feature(core_intrinsics)]
#![feature(used)]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_rt;
extern crate stm32f30x;

mod led;

use core::intrinsics::{volatile_load, volatile_store};
use cortex_m::{asm, exception};
use cortex_m::peripheral::{SCB, SYST};
use stm32f30x::{GPIOD, RCC};

use led::*;
use led::Led::*;

const BUTTONS: usize = 4;
const BUTTON_LED: [Led; BUTTONS] = [ LD8, LD10, LD9, LD7 ];
const BUTTON_PIN: [usize; BUTTONS] = [ 12, 13, 14, 15 ];
static mut BUTTON_CHANGED: [bool; BUTTONS] = [ false, false, false, false];
static mut BUTTON_STATE: [bool; BUTTONS] = [ false, false, false, false];
static mut BUTTON_DEBOUNCE: [u32; BUTTONS] = [ 0, 0, 0, 0 ];

#[inline(never)]
fn main() {
    cortex_m::interrupt::free(|cs| {
        // borrow peripherals
        let rcc = RCC.borrow(cs);
        let syst = SYST.borrow(cs);
        let scb = SCB.borrow(cs);
        let gpiod = GPIOD.borrow(cs);

        // power on GPIOD and GPIOE
        rcc.ahbenr.modify(|_, w| w.iopden().enabled()
                                  .iopeen().enabled());

        // initialize LEDs
        led_init(LD3);
        led_init(LD4);
        led_init(LD7);
        led_init(LD8);
        led_init(LD9);
        led_init(LD10);

        // enable Cortex-M SysTick counter
        syst.set_reload(8000); // set to update every 8000 clocks, or every 1ms
        unsafe { scb.shpr[11].write(0xf0); } // set SysTick exception (interrupt) priority to lowest possible
        syst.clear_current();
        // -FIX- SVD has incorrect identifiers here, so the API is nonsensical:
        unsafe { syst.csr.write(0b100); } // set clock to AHB, not AHB/8
        syst.enable_interrupt();
        syst.enable_counter();

        // configure GPIOD button input pins
        gpiod.moder.modify(|_, w| w.moder12().input()
                                   .moder13().input()
                                   .moder14().input()
                                   .moder15().input());
        unsafe {
            gpiod.pupdr.modify(|_, w| w.pupdr12().bits(0b01) // pull up
                                       .pupdr13().bits(0b01)
                                       .pupdr14().bits(0b01)
                                       .pupdr15().bits(0b01));
        }

        // turn on LD4 (northwest, blue) to show we've gotten this far
        led_on(LD4);
    });

    loop {
        led_toggle(LD3); // heartbeat
        delay_ms(100);
        for i in 0..BUTTONS {
            unsafe {
                if volatile_load(&BUTTON_CHANGED[i]) {
                    volatile_store(&mut BUTTON_CHANGED[i], false);
                    led_set(BUTTON_LED[i], volatile_load(&BUTTON_STATE[i]));
                }
            }
        }
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
    unsafe {
        // decrement delay
        if TIMING_DELAY != 0 {
            TIMING_DELAY -= 1;
        }

        // read the buttons, with debounce
        for i in 0..BUTTONS {
            if BUTTON_DEBOUNCE[i] > 0 {
                BUTTON_DEBOUNCE[i] -= 1;
            } else {
                let gpiod = GPIOD.get();
                // buttons are short-to-ground-with-pull-up, so invert the logic
                let state = ((*gpiod).idr.read().bits() & (1 << BUTTON_PIN[i])) == 0;
                if state {
                    if BUTTON_STATE[i] == false {
                        BUTTON_STATE[i] = true;
                        BUTTON_CHANGED[i] = true;
                        BUTTON_DEBOUNCE[i] = 100;
                    }
                } else {
                    if BUTTON_STATE[i] == true {
                        BUTTON_STATE[i] = false;
                        BUTTON_CHANGED[i] = true;
                        BUTTON_DEBOUNCE[i] = 100;
                    }
                }
            }
        }
    }
}

fn delay_ms(ms: u32) {
    unsafe {
        volatile_store(&mut TIMING_DELAY, ms);
        while volatile_load(&TIMING_DELAY) != 0 {}
    }
}

#[allow(dead_code)]
#[used]
#[link_section = ".rodata.interrupts"]
static INTERRUPTS: [extern "C" fn(); 240] = [default_handler; 240];

extern "C" fn default_handler() {
    asm::bkpt();
}
