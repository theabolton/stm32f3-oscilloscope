// sysclk.rs -- configure the STM32F303 system clock and flash for 72MHz operation

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

extern crate cortex_m;

use cortex_m::asm;
use stm32f30x::{FLASH, RCC};

// set_sys_clock()
// Set the system clock to 72MHz, using the 8MHz external clock from ST-Link.
// This assumes the clock and PLL are still in their reset state, and turns
// off the HSI clock when no longer needed, but otherwise follows the
// STM32F3-Discovery_FW_V1.1.0 library procedure.
pub fn set_sys_clock() {
    cortex_m::interrupt::free(|cs| {
        let rcc = RCC.borrow(cs);
        let flash = FLASH.borrow(cs);

        // turn on HSE with bypass
        rcc.cr.modify(|_, w| unsafe { w.hseon().bits(1)
                                       .hsebyp().bits(1) });
        // wait for HSE to become ready
        let mut startup_count = 0x500;
        while rcc.cr.read().hserdy().bits() == 0 {
            startup_count -= 1;
            if startup_count == 0 {
                asm::bkpt();  // HSE did not become ready; halt
            }
        }
        // set flash prefetch and latency
        flash.acr.modify(|_, w| unsafe { w.prftbe().bits(1)
                                          .latency().bits(0b010) });
        // set bus clocks
        rcc.cfgr.modify(|_, w| unsafe {
             w.hpre().bits(0) // HCLK = SYSCLK
             .ppre2().bits(0) // PCLK2 = HCLK
             .ppre1().bits(0b100) // PCLK1 = HCLK / 2
        });
        // set PLL for 9 times HSE input
        rcc.cfgr.modify(|_, w| unsafe {
            w.pllsrc().bits(1) // PLL source HSE/PREDIV
            .pllmul().bits(0b0111) // PLL multiplier 9
        });
        // enable PLL and wait for it to ready
        rcc.cr.modify(|_, w| unsafe { w.pllon().bits(1) });
        while rcc.cr.read().pllrdy().bits() == 0 {}
        // select PLL as system clock
        rcc.cfgr.modify(|_, w| unsafe { w.sw().bits(0b10) });
        // wait until PLL is used
        while rcc.cfgr.read().sws().bits() != 0b10 {}
        // turn off HSI
        rcc.cr.modify(|_, w| unsafe { w.hsion().bits(0) });
    });
}
