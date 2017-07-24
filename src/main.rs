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

// For a summary of the peripherals used, see docs/peripherals.rst

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
use cortex_m::exception;
use cortex_m::peripheral::{SCB, SYST, SystClkSource};
use stm32f30x::{DMA1, GPIOD, RCC, interrupt};

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
    st7735_fill_rect,
};

// ======== global (cough) state ========

// constants and state for the LCD breakout board pushbuttons
const BUTTONS: usize = 4;
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
    SiggenFreq { frequency:     1, label: b"1Hz" },
    SiggenFreq { frequency:     3, label: b"3Hz" },
    SiggenFreq { frequency:    10, label: b"10Hz" },
    SiggenFreq { frequency:    33, label: b"33Hz" },
    SiggenFreq { frequency:   100, label: b"100Hz" },
    SiggenFreq { frequency:   333, label: b"333Hz" },
    SiggenFreq { frequency:  1000, label: b"1kHz" },
    SiggenFreq { frequency:  3333, label: b"3.3kHz" },
    SiggenFreq { frequency: 10000, label: b"10kHz" },
];

// timebase intervals
struct TimebaseInterval {
    sample_rate: u32, // samples per second
    label: &'static [u8],
}

const TIMEBASE_INTERVALS: [TimebaseInterval; 17] = [
    TimebaseInterval { sample_rate:       1, label: b"32s" },
    TimebaseInterval { sample_rate:      32, label: b"1s" },
    TimebaseInterval { sample_rate:      64, label: b".5s" },
    TimebaseInterval { sample_rate:     160, label: b".2s" },
    TimebaseInterval { sample_rate:     320, label: b".1s" },
    TimebaseInterval { sample_rate:     640, label: b"50ms" },
    TimebaseInterval { sample_rate:    1600, label: b"20ms" },
    TimebaseInterval { sample_rate:    3200, label: b"10ms" },
    TimebaseInterval { sample_rate:    6400, label: b"5ms" },
    TimebaseInterval { sample_rate:   16000, label: b"2ms" },
    TimebaseInterval { sample_rate:   32000, label: b"1ms" },
    TimebaseInterval { sample_rate:   64000, label: b".5ms" },
    TimebaseInterval { sample_rate:  160000, label: b".2ms" },
    TimebaseInterval { sample_rate:  320000, label: b".1ms" },
    TimebaseInterval { sample_rate:  640000, label: b"~50us" }, // 49.777µs/div
    TimebaseInterval { sample_rate: 1600000, label: b"20us" },
    TimebaseInterval { sample_rate: 3130434, label: b"~10us" }, // 10.222µs/div
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
        led_init(LD5);

        // enable Cortex-M SysTick counter
        syst.set_reload(9000); // set to update every 9000 clocks, or every 1ms
        // - set SysTick exception (interrupt) priority to lowest possible
        unsafe { scb.shpr[11].write(0xf0); } // write to PRI_15 field of SHPR3
        syst.clear_current();
        // SVD has incorrect identifiers here, so the API is nonsensical:
        // 'Core' means AHB, 'External' means AHB/8
        syst.set_clock_source(SystClkSource::External);
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
    st7735_print(b"stm-scope", 0, 0, St7735Color::Green, St7735Color::Black);
    //st7735_print(env!("CARGO_PKG_VERSION").as_ref(),
    //             10 * 8, 0, St7735Color::Green, St7735Color::Black);

    // signal generator (DAC, DMA, TIM, GPIO output) setup
    siggen_setup();

    // capture (ADC, DMA, TIM, GPIO input) setup
    capture::setup();

    // turn on LD4 (northwest, blue) to show we've gotten this far
    led_on(LD4);

    // paint graticule
    let mut x = 32;
    while x <= 128 {
        let mut y = 32;
        while y <= 96 {
            st7735_drawPixel(x, 127 - y, St7735Color::Red as u16);
            y += 32;
        }
        x += 32;
    }

    // ======== main loop ========

    enum SweepState {
        Before, // timer running, but capture not started
        During, // capture running or finished, display in progress
        After,  // capture and display finished
    };
    let mut state = SweepState::Before;

    let mut siggen_freq_index = 6; // 1kHz
    set_siggen_freq_from_index(siggen_freq_index);
    let mut timebase_index = TIMEBASE_INTERVALS.len() / 2; // -FIX- something in the middle
    set_capture_timebase_from_index(timebase_index);
    let mut previous_y = [255u8; 160];
    let mut x_out = 0;

    loop {
        match state {
            SweepState::Before => {
                // begin the next sweep of 160 samples
                capture::begin_sweep();
                // turn on LD3 at the beginning of the capture sweep
                led_on(LD3);
                state = SweepState::During;
                x_out = 0;
            }
            SweepState::During => {
                // Plot data as it becomes available via DMA from ADC1
                // - read the number of samples transfered by DMA controller
                let x_in = capture::get_transferred_sample_count();
                if x_in > x_out {
                    // erase old plot
                    let x = x_out as i16;
                    let y = previous_y[x_out] as i16;
                    if y < 255 {
                        if x % 32 == 0 && y % 32 == 0 {
                            st7735_drawPixel(x, y, St7735Color::Green as u16);
                        } else {
                            st7735_drawPixel(x, y, St7735Color::Black as u16);
                        }
                    }
                    // plot new value
                    let raw_conversion = capture::channel_1_data()[x_out];
                    let microvolts_per_lsb = 806u32; // 3.3v / 2^12 bits * 10^6
                    let microvolts = raw_conversion as u32 * microvolts_per_lsb;
                    // Note that the 3.3v * 10^6 just cancels out in these calculations; we could
                    // just right shift by 5 bits. But later we'll want the vertical gain
                    // represented in terms of voltage, so build it in now.
                    let microvolts_per_y = 25_781u32; // 3.3v * 10^6 / 128 pixels
                    let y = 127 - (microvolts / microvolts_per_y) as i16;
                    if y < 0 { // (can't yet happen)
                        st7735_drawPixel(x, 0, St7735Color::Red as u16);
                        previous_y[x_out] = 0;
                    } else if y > 127 {
                        st7735_drawPixel(x, 127, St7735Color::Red as u16);
                        previous_y[x_out] = 127;
                    } else {
                        st7735_drawPixel(x, y, St7735Color::White as u16);
                        previous_y[x_out] = y as u8;
                    }
                    // end of sweep?
                    x_out += 1;
                    if x_out >= 160 {
                        state = SweepState::After;
                    }
                }
            }
            SweepState::After => {
                // Sweep is finished (both capture and display)
                // - disable DMA and prepare for next sweep
                capture::finish_sweep();
                if capture::check_adc_ovr_flag() {
                    #[cfg(debug_assertions)]
                    st7735_print(b"OVR set", 0, 104, St7735Color::Green, St7735Color::Black);
                }
                // toggle LD5 at the end of each display sweep
                led_toggle(LD5);
                state = SweepState::Before;
            }
        }

        // button 1 (left): change timebase
        if button_get_changed(0) {
            button_reset_changed(0);
            if button_get_state(0) {
                timebase_index = (timebase_index + 1) % TIMEBASE_INTERVALS.len();
                set_capture_timebase_from_index(timebase_index);
            }
        }
        // button 4 (right): change signal generator frequency
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
    clear_status_line();
    st7735_print(b"siggen freq:", 0, 116, St7735Color::Green, St7735Color::Black);
    st7735_print(f.label, 104, 116, St7735Color::Green, St7735Color::Black);
}

fn set_capture_timebase_from_index(i: usize) {
    let t = &TIMEBASE_INTERVALS[i];
    capture::set_timebase(t.sample_rate);
    clear_status_line();
    st7735_print(t.label, 0, 116, St7735Color::Green, St7735Color::Black);
    st7735_print(b"/div", 8 * t.label.len() as u8, 116, St7735Color::Green, St7735Color::Black);
}

fn clear_status_line() {
    st7735_fill_rect(0, 116, 160, 12, St7735Color::Black as u16);
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
static INTERRUPTS: interrupt::Handlers = interrupt::Handlers {
    Dma1Ch1: dma1ch1_interrupt_handler,
    ..interrupt::DEFAULT_HANDLERS
};

extern "C" fn dma1ch1_interrupt_handler(_ctxt: interrupt::Dma1Ch1) {
    // turn off LD3 at the end of the capture sweep
    led_off(LD3);
    // clear the DMA1 channel 1 transfer complete interrupt flag TCIF
    let dma1 = DMA1.get();
    unsafe { (*dma1).ifcr.write(|w| w.ctcif1().bits(1)); }
}
