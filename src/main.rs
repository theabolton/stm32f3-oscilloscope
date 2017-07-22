// stm32f3-oscilloscope - src/main.rs

// A low-bandwidth, digital storage oscilloscope for the STM32F3 Discovery
// development board.

// Copyright © 2017 Sean Bolton
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
// LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
// WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

// STM32F3 to ST7735 display breakout board pushbuttons:
// J1-16       GND          black   GND
// J1-17   Button4 (right)  brown   PD15
// J1-18   Button3          white   PD14
// J1-19   Button2          grey    PD13
// J1-20   Button1 (left)   purple  PD12

// LEDs:
// LD3 (N, red)     - out-of-reset indication
// LD4 (NW, blue)   - initialization completed
// LD5 (NE, orange) - toggled after each LCD sweep

// x LD               - heartbeat (main loop)
// x LD8 (SW, orange) - Button1 (left)
// x LD10 (S, red)    - Button2
// x LD9 (SE, blue)   - Button3
// x LD7 (E, green)   - Button4 (right)

#![feature(core_intrinsics)]
#![feature(used)]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_rt;
extern crate stm32f30x;

mod capture;
mod led;
mod parallax_8x12_font;
mod siggen;
mod st7735;
mod sysclk;

use core::intrinsics::{volatile_load, volatile_store};
use cortex_m::{asm, exception};
use cortex_m::peripheral::{SCB, SYST};
use stm32f30x::{GPIOD, RCC};

use capture::*;
use led::*;
use led::Led::*;
use siggen::*;
use st7735::*;
use sysclk::set_sys_clock;

// ======== required declarations for Rust and C linkage ========

// the C functions we call from Rust
extern "C" {
    fn _st7735_initR(lcd_type: u8);
    fn _st7735_drawFastHLine(x: i16, y: i16, h: i16, color: u16);
    fn _st7735_drawFastVLine(x: i16, y: i16, h: i16, color: u16);
    fn _st7735_drawPixel(x: i16, y: i16, color: u16);
    fn _st7735_fillScreen(color: u16);
    fn _st7735_pushColor(color: u16);
    fn _st7735_setAddrWindow(x0: u8, y0: u8, x1: u8, y1: u8);
    fn _st7735_setRotation(rotation: u8);
    fn _st7735_get_height() -> u8;
    fn _st7735_get_width() -> u8;
}

// the Rust functions in submodules that we call from C
pub use st7735::{
    st7735_send_cmd,
    st7735_send_data,
    lcd_cs0, lcd_cs1,
    lcd_rst0, lcd_rst1,
};

// ======== global (cough) state ========

// constants and state for the LCD breakout board pushbuttons
const BUTTONS: usize = 4;
const BUTTON_LED: [Led; BUTTONS] = [ LD8, LD10, LD9, LD7 ];
const BUTTON_PIN: [usize; BUTTONS] = [ 12, 13, 14, 15 ];
static mut BUTTON_CHANGED: [bool; BUTTONS] = [ false, false, false, false];
static mut BUTTON_STATE: [bool; BUTTONS] = [ false, false, false, false];
static mut BUTTON_DEBOUNCE: [u32; BUTTONS] = [ 0, 0, 0, 0 ];

fn button_get_changed(i: usize) -> bool {
    unsafe { volatile_load(&BUTTON_CHANGED[i]) }
}
fn button_reset_changed(i: usize) {
    unsafe { volatile_store(&mut BUTTON_CHANGED[i], false); }
}
fn button_get_state(i: usize) -> bool {
    unsafe { volatile_load(&BUTTON_STATE[i]) }
}

// ======== constants ========

// signal generator frequencies
struct SiggenFreq {
    frequency: u32,
    label: &'static [u8],
}

const SIGGEN_FREQUENCIES: [SiggenFreq; 9] = [
    // -FIX- label abbreviations are ugly
    SiggenFreq { frequency:     1, label: b" 1Hz" },
    SiggenFreq { frequency:     3, label: b" 3Hz" },
    SiggenFreq { frequency:    10, label: b"10Hz" },
    SiggenFreq { frequency:    33, label: b"33Hz" },
    SiggenFreq { frequency:   100, label: b"100H" },
    SiggenFreq { frequency:   333, label: b"333H" },
    SiggenFreq { frequency:  1000, label: b"1kHz" },
    SiggenFreq { frequency:  3333, label: b"3.3k" },
    SiggenFreq { frequency: 10000, label: b"10kH" },
];

// ======== main ========

#[inline(never)]
fn main() {
    // set system clock to 72MHz
    set_sys_clock();

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

        // set up LCD breakout board pushbuttons
        // - GPIOD powered on above
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
    });

    // LCD setup
    st7735_setup();
    delay_ms(50);
    st7735_initR(St7735Type::RedTab as u8);
    st7735_setRotation(3); // landscape
    st7735_fillScreen(St7735Color::Black as u16);
    st7735_print(0, 0, b"stm-scope", St7735Color::Green, St7735Color::Black);
    //st7735_print(10 * 8, 0, env!("CARGO_PKG_VERSION").as_ref(),
    //             St7735Color::Green, St7735Color::Black);
    // -FIX- also available;
    // st7735_drawPixel(0, 0, St7735Color::White as u16);
    // st7735_drawFastVLine(10, 0, 10, St7735Color::Blue as u16);

    // signal generator (DAC, DMA, TIM, GPIO output) setup
    siggen_setup();

    // capture (ADC, DMA, TIM, GPIO input) setup
    capture_setup();

    // turn on LD4 (northwest, blue) to show we've gotten this far
    led_on(LD4);

    // ======== main loop ========

    let mut siggen_freq_index = 6; // 1kHz
    set_siggen_freq_from_index(siggen_freq_index);
    loop {
        led_toggle(LD3); // heartbeat
        delay_ms(100);
        for i in 0..3 {
            if button_get_changed(i) {
                button_reset_changed(i);
                led_set(BUTTON_LED[i], button_get_state(i));
            }
        }
        // button 3: change signal generator frequency
        if button_get_changed(3) {
            button_reset_changed(3);
            if button_get_state(3) {
                siggen_freq_index = (siggen_freq_index + 1) % SIGGEN_FREQUENCIES.len();
                set_siggen_freq_from_index(siggen_freq_index);
            }
        }
    }
}

fn set_siggen_freq_from_index(i: usize) {
    let f = &SIGGEN_FREQUENCIES[i];
    siggen_set_freq(f.frequency);
    st7735_print(128, 116, f.label, St7735Color::Green, St7735Color::Black);
}

// ======== exception handlers, including SysTick ========

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

#[no_mangle]
pub extern "C" fn delay_ms(ms: u32) {
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
