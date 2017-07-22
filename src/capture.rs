// stm32f3-oscilloscope - src/capture.rs
// input capture, using ADC1, DMA?, TIM? from inputs on PC1 and PC?

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

use cortex_m;
use stm32f30x::{ADC1, ADC1_2, GPIOC, RCC};

use delay_ms;

pub fn capture_setup() {
    cortex_m::interrupt::free(|cs| {
        // enable clock to ADC1 and GPIOC
        let rcc = RCC.borrow(cs);
        rcc.ahbenr.modify(|_, w| w.adc12en().enabled()
                                  .iopcen().enabled());
//        // enable clock to DAC1 and TIM2
//        rcc.apb1enr.modify(|_, w| w.dacen().enabled()
//                                   .tim2en().enabled());

        // configure PC1 and PC? as analog inputs with no pull
        let gpioc = GPIOC.borrow(cs);
        gpioc.moder.modify(|_, w|
            w.moder1().analog()
        );
        gpioc.pupdr.modify(|_, w| unsafe {
            w.pupdr1().bits(0b00) // no pull
        });

        // configure ADC clock
        // -FIX- adjust sample time with sample rate
        rcc.cfgr2.modify(|_, w| unsafe { w.adc12pres().bits(0b10001) }); // ADC clock is PLL/2

        // ADC calibration procedure
        // - turn on voltage regulator
        let adc1 = ADC1.borrow(cs);
        adc1.cr.modify(|_, w| unsafe { w.advregen().bits(0b00) }); // set to intermediate state first
        adc1.cr.modify(|_, w| unsafe { w.advregen().bits(0b01) }); // then enable
        // - leave critical section and wait for at least 10µs (the hardware requirement)
    });
        delay_ms(2); // delay at least 1ms (convenient, but longer than required)
        // - enter critical section again
    cortex_m::interrupt::free(|cs| {
        // - select calibration mode
        let adc1 = ADC1.borrow(cs);
        adc1.cr.modify(|_, w| unsafe { w.adcaldif().bits(0) }); // single-ended
        // - start calibration
        adc1.cr.modify(|_, w| unsafe { w.adcal().bits(1) });
        // - wait for calibration to finish
        while adc1.cr.read().adcal().bits() != 0 {}
        // - calibration complete

        // configure ADC1 for manual sampling -FIX-
        let adc12 = ADC1_2.borrow(cs);
        adc12.ccr.modify(|_, w| unsafe {
            w.ckmode().bits(0b00) // async clock
             .mdma().bits(0b00)   // DMA disabled -FIX-
             .dmacfg().bits(0)    // one-shot mode
             .delay().bits(0)     // no delay between phases (for interleaved mode only)
             .mult().bits(0)      // independent mode -FIX- for dual channel
        });
        adc1.cfgr.modify(|_, w| unsafe {
            w.jauto().bits(0)       // no auto inject group conversion
             .cont().bits(1)        // continuous conversion !FIX! what?
             .ovrmod().bits(0)      // keep old value on overrun
             .exten().bits(0b00)    // external trigger detection disabled
             .extsel().bits(0b0000) // external trigger event 0
             .align().bits(0)       // align right
             .res().bits(0b00)      // 12 bits
        });
        adc1.sqr1.modify(|_, w| unsafe {
            w.sq1().bits(7)     // 1st conversion in sequence: channel 7
             .l3().bits(0b0000) // 1 conversion in sequence  (typo in SVD, should be "l")
        });
        adc1.smpr1.modify(|_, w| unsafe { w.smp7().bits(0b011) }); // sample time 7.5 cycles -FIX-

        // enable ADC1
        adc1.cr.modify(|_, w| unsafe { w.aden().bits(1) });
        // wait for ADRDY
        while adc1.isr.read().adrdy().bits() == 0 {}
    });
}
