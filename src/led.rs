// led.rs
// BSP for STM32F3 Discovery LEDs
// Assumes that GPIOE has alread been powered up.

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

// extern crate stm32f30x;

use stm32f30x::GPIOE;

#[derive(Clone, Copy)]
#[allow(unused)]
pub enum Led {
//  Northwest  North  NE      East   SE    South  SW      West
//  blue       red    orange  green  blue  red    orange  green
    LD4 = 8,   LD3,   LD5,    LD7,   LD9,  LD10,  LD8,    LD6,
}

pub fn led_init(led: Led) {
    let pin = led as u8;
    if pin >= 8 && pin <= 15 {
        let gpioe = GPIOE.get();
        // set pin for output
        let mask = !(0b11 << (pin * 2));
        let mode = 0b01 << (pin * 2);
        unsafe { (*gpioe).moder.modify(|r, w| w.bits((r.bits() & mask) | mode)); }
    }
}

#[allow(unused)]
pub fn led_set(led: Led, state: bool) {
    let pin = led as u8;
    if pin >= 8 && pin <= 15 {
        let gpioe = GPIOE.get();
        if state {
            unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << pin)); }
        } else {
            unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << (pin + 16))); }
        }
    }
}

#[allow(unused)]
pub fn led_on(led: Led) {
    let pin = led as u8;
    if pin >= 8 && pin <= 15 {
        let gpioe = GPIOE.get();
        unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << pin)); }
    }
}

#[allow(unused)]
pub fn led_off(led: Led) {
    let pin = led as u8;
    if pin >= 8 && pin <= 15 {
        let gpioe = GPIOE.get();
        unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << (pin + 16))); }
    }
}

#[allow(unused)]
pub fn led_toggle(led: Led) {
    let pin = led as u8;
    if pin >= 8 && pin <= 15 {
        let gpioe = GPIOE.get();
        let new_state = ( unsafe { (*gpioe).odr.read().bits() } & (1 << pin)) == 0;
        if new_state {
            unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << pin)); }
        } else {
            unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << (pin + 16))); }
        }
    }
}
