// stm32f3-oscilloscope - src/capture.rs
// input capture, using ADC1, DMA1, TIM15 from input on PC1

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

// This uses ADC1 channel 7, input on GPIO PC1
// - ADC12 is clocked by AHB clock to minimize jitter
// TIM15 triggers the ADC conversions
// - this is a 16-bit counter with 16-bit prescaler, clocked directly from APB2 (72MHz)
// - TIM15 outputs TIM15_TRGO, which is ADC1's EXT14
// DMA1 channel 1 moves converted data to RAM

use cortex_m;
use stm32f30x::{ADC1, ADC1_2, DMA1, GPIOC, RCC, TIM15};
use stm32f30x::interrupt::Interrupt;

use delay_ms;

pub static mut CAPTURE_CHANNEL_1: [u16; 160] = [0; 160];

/// Prepares the hardware for sample capture, by configuring the ADC, timer, DMA channel, and
/// GPIO pin. Each of those peripherals will be ready for a new sampling sweep, except for the
/// ADC start and DMA enabling, which is done by `begin_sweep`.
pub fn setup() {
    cortex_m::interrupt::free(|cs| {
        // enable clock to ADC1, DMA1, and GPIOC
        let rcc = RCC.borrow(cs);
        rcc.ahbenr.modify(|_, w|
            w.adc12en().enabled()
             .dmaen().enabled() // should be 'dma1en'
             .iopcen().enabled()
        );
        // enable clock to TIM15
        rcc.apb2enr.modify(|_, w| w.tim15en().enabled());

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
        // - turn off the PLL-based ADC12 clock
        rcc.cfgr2.modify(|_, w| unsafe { w.adc12pres().bits(0b00000) }); // ADC clock is from AHB
        // - turn on the AHB clock to ADC12, set to AHB/2
        //   (calibration will hang if this isn't done now)
        let adc12 = ADC1_2.borrow(cs);
        adc12.ccr.modify(|_, w| unsafe { w.ckmode().bits(0b10) });

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

        // configure ADC1 for TIM15-driven sampling
        let adc12 = ADC1_2.borrow(cs);
        adc12.ccr.modify(|_, w| unsafe {
            w.ckmode().bits(0b10) // ADC clock is AHB/2
             .mdma().bits(0b00)   // dual DMA mode: disabled
             .dmacfg().bits(0)    // dual DMA mode: one-shot
             .delay().bits(0)     // no delay between phases (for interleaved mode only)
             .mult().bits(0)      // independent mode -FIX- for dual channel
        });
        adc1.cfgr.modify(|_, w| unsafe {
            w.jauto().bits(0)       // no auto inject group conversion
             .cont().bits(0)        // single (non-continuous) conversion mode
             .ovrmod().bits(1)      // keep new value on overrun
             .exten().bits(0b01)    // external trigger on rising edge
             .extsel().bits(0b1110) // external trigger is EXT14: TIM15_TRGO
             .align().bits(0)       // align right
             .res().bits(0b00)      // 12 bits
             .dmacfg().bits(0)      // DMA one-shot mode
             .dmaen().bits(1)       // DMA enabled
        });
        adc1.sqr1.modify(|_, w| unsafe {
            w.sq1().bits(7)     // 1st conversion in sequence: channel 7
             .l3().bits(0b0000) // 1 conversion in sequence  (typo in SVD, should be "l", not "l3")
        });
        adc1.smpr1.modify(|_, w| unsafe { w.smp7().bits(0b011) }); // sample time 7.5 cycles -FIX-

        // configure TIM15 to trigger sampling
        let tim15 = TIM15.borrow(cs);
        tim15.cr1.modify(|_, w| unsafe {
            w.ckd().bits(0b00) // no sampling filter clock division
             .arpe().bits(1)   // ARR register is buffered
        });
        tim15.cr2.modify(|_, w| unsafe { w.mms().bits(0b010) }); // trigger output: update event
        tim15.arr.write(|w| unsafe { w.bits(999) }); // 1kHz
        tim15.psc.write(|w| unsafe { w.psc().bits(71) }); // prescaler of 72
        tim15.egr.write(|w| unsafe { w.ug().bits(1) }); // immediately update registers

        // configure DMA1 channel 1 for ADC1
        // - assuming reset state
        let dma1 = DMA1.borrow(cs);
        dma1.ccr1.modify(|_, w| unsafe {
            w.mem2mem().bits(0)  // memory-to-memory mode disabled
             .pl().bits(0b10)    // high priority
             .msize().bits(0b01) // memory data size 16 bits
             .psize().bits(0b01) // peripheral data size 16 bits
             .minc().bits(1)     // memory increment enabled
             .pinc().bits(0)     // peripheral increment disabled
             .circ().bits(0)     // one-shot (not circular) mode
             .dir().bits(0)      // transfer direction: peripheral -> memory
             .tcie().bits(1)     // trigger interrupt on transfer completion
        });
        dma1.cndtr1.write(|w| unsafe { w.ndt().bits(160) });  // buffer size
        let adc1_dr_address: u32 = &adc1.dr as *const _ as u32;
        debug_assert_eq!(adc1_dr_address, 0x50000040);
        dma1.cpar1.write(|w| unsafe {
            w.bits(adc1_dr_address) // peripheral base address
        });
        dma1.cmar1.write(|w| unsafe {
            w.bits(&CAPTURE_CHANNEL_1 as *const _ as u32) // memory base address
        });
        // - enable DMA1_Channel1 interrupt
        let nvic = cortex_m::peripheral::NVIC.borrow(cs);
        unsafe { nvic.set_priority(Interrupt::Dma1Ch1, 0); }
        nvic.enable(Interrupt::Dma1Ch1);

        // enable ADC1
        adc1.cr.modify(|_, w| unsafe { w.aden().bits(1) });
        // wait for ADRDY
        while adc1.isr.read().adrdy().bits() == 0 {}

        // enable TIM15
        tim15.cr1.modify(|_, w| unsafe { w.cen().bits(1) });
    });
}

/// Begins a new sampling sweep by enabling DMA and starting ADC conversions.
pub fn begin_sweep() {
    // begin the next sweep of 160 samples
    cortex_m::interrupt::free(|cs| {
        // enable DMA
        let dma1 = DMA1.borrow(cs);
        unsafe { dma1.ccr1.modify(|_, w| w.en().bits(1)); }
        // start ADC conversions (timer is already running)
        let adc1 = ADC1.borrow(cs);
        unsafe { (*adc1).cr.modify(|_, w| w.adstart().bits(1)); }
    });
}

/// Returns the number of samples transferred by DMA to RAM.
pub fn get_transferred_sample_count() -> usize {
    let dma1 = DMA1.get();
    160 - unsafe { (*dma1).cndtr1.read().ndt().bits() } as usize
}

/// Returns a reference to the sampled data for channel 1. Use `get_transferred_sample_count()` to
/// determine how many samples are valid.
pub fn channel_1_data() -> &'static [u16] {
    unsafe { &CAPTURE_CHANNEL_1 }
}

/// Turns off DMA and prepares for the next sweep.
pub fn finish_sweep() {
    let dma1 = DMA1.get();
    // - disable DMA
    unsafe { (*dma1).ccr1.modify(|_, w| w.en().bits(0)); }
    // - re-set transfer length
    unsafe { (*dma1).cndtr1.write(|w| w.ndt().bits(160)); }
}

/// Checks the AC OVR overrun flag, and clears it if set. Returns its value before it was cleared.
pub fn check_adc_ovr_flag() -> bool {
    // test and return ADC OVR flag
    let adc1 = ADC1.get();
    let ovr = unsafe { (*adc1).isr.read().ovr().bits() } != 0;
    if ovr {
        // - OVR was set, clear it
        unsafe { (*adc1).isr.modify(|_, w| w.ovr().bits(1)); }
    }
    ovr
}

/// Sets the timebase for sampling, to the specified number of samples per second.
/// This sets the TIM15 update rate, and -FIX- should set the sample time as well, but doesn't yet.
// -FIX- this works well out to 1 sample per second, but it might be cool to implement very long
// sample intervals, e.g. one sample per minute or more.
pub fn set_timebase(samples_per_second: u32) {
    let arr;
    let psc;
    if samples_per_second > 1097 {
        arr = 72_000_000 / samples_per_second - 1;
        psc = 0;
    } else {
        arr = (72_000_000 / 2250) / samples_per_second - 1;
        psc = 2249;
    }
    cortex_m::interrupt::free(|cs| {
        let tim15 = TIM15.borrow(cs);
        tim15.arr.write(|w| unsafe { w.bits(arr) });
        tim15.psc.write(|w| unsafe { w.psc().bits(psc) });
        tim15.cnt.write(|w| unsafe { w.cnt().bits(0) });
        tim15.egr.write(|w| unsafe { w.ug().bits(1) }); // immediately update registers
    });
}
