// led.rs
// BSP for STM32F3 Discovery LEDs
// Assumes that GPIOE has alread been powered up.

// extern crate stm32f30x;

use stm32f30x::GPIOE;

#[derive(Clone, Copy)]
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

pub fn led_on(led: Led) {
    let pin = led as u8;
    if pin >= 8 && pin <= 15 {
        let gpioe = GPIOE.get();
        unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << pin)); }
    }
}

pub fn led_off(led: Led) {
    let pin = led as u8;
    if pin >= 8 && pin <= 15 {
        let gpioe = GPIOE.get();
        unsafe { (*gpioe).bsrr.write(|w| w.bits(1 << (pin + 16))); }
    }
}

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
