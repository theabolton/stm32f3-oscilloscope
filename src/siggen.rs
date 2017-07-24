// stm32f3-oscilloscope - src/siggen.rs
// signal generator, using DAC1, DMA2, TIM2, output on PA4 and PA5

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

use core;

use cortex_m;
use stm32f30x::{DAC, DMA2, GPIOA, RCC, TIM2};

/* With 72- or 144-sample tables, output can be set to exactly 1Hz, 10Hz, 1kHz, etc. */
const SINE_12BIT: [u16; 144] = [
    2047, 2136, 2225, 2314, 2402, 2490, 2577, 2663, 2747, 2830, 2912, 2992, 3071, 3147, 3221, 3293, 
    3363, 3430, 3494, 3556, 3615, 3671, 3724, 3773, 3820, 3863, 3902, 3938, 3971, 3999, 4024, 4045, 
    4063, 4076, 4086, 4092, 4094, 4092, 4086, 4076, 4063, 4045, 4024, 3999, 3971, 3938, 3902, 3863, 
    3820, 3773, 3724, 3671, 3615, 3556, 3494, 3430, 3363, 3293, 3221, 3147, 3071, 2992, 2912, 2830, 
    2747, 2663, 2577, 2490, 2402, 2314, 2225, 2136, 2047, 1958, 1869, 1780, 1692, 1604, 1517, 1431, 
    1347, 1264, 1182, 1102, 1023,  947,  873,  801,  731,  664,  600,  538,  479,  423,  370,  321, 
     274,  231,  192,  156,  123,   95,   70,   49,   31,   18,    8,    2,    0,    2,    8,   18, 
      31,   49,   70,   95,  123,  156,  192,  231,  274,  321,  370,  423,  479,  538,  600,  664, 
     731,  801,  873,  947, 1024, 1102, 1182, 1264, 1347, 1431, 1517, 1604, 1692, 1780, 1869, 1958, 
];

const RAMP_8BIT: [u8; 144] = [
      0,   2,   4,   5,   7,   9,  11,  12,  14,  16,  18,  20,  21,  23,  25,  27, 
     29,  30,  32,  34,  36,  37,  39,  41,  43,  45,  46,  48,  50,  52,  53,  55, 
     57,  59,  61,  62,  64,  66,  68,  70,  71,  73,  75,  77,  78,  80,  82,  84, 
     86,  87,  89,  91,  93,  95,  96,  98, 100, 102, 103, 105, 107, 109, 111, 112, 
    114, 116, 118, 119, 121, 123, 125, 127, 128, 130, 132, 134, 136, 137, 139, 141, 
    143, 144, 146, 148, 150, 152, 153, 155, 157, 159, 160, 162, 164, 166, 168, 169, 
    171, 173, 175, 177, 178, 180, 182, 184, 185, 187, 189, 191, 193, 194, 196, 198, 
    200, 202, 203, 205, 207, 209, 210, 212, 214, 216, 218, 219, 221, 223, 225, 226, 
    228, 230, 232, 234, 235, 237, 239, 241, 243, 244, 246, 248, 250, 251, 253, 255, 
];

pub fn siggen_setup() {
    cortex_m::interrupt::free(|cs| {
        let rcc = RCC.borrow(cs);
        let gpioa = GPIOA.borrow(cs);
        let tim2 = TIM2.borrow(cs);
        let dac = DAC.borrow(cs);
        let dma2 = DMA2.borrow(cs);

        // enable clock to DMA2 and GPIOA
        rcc.ahbenr.modify(|_, w| w.dma2en().enabled()
                                  .iopaen().enabled());
        // enable clock to DAC1 and TIM2
        rcc.apb1enr.modify(|_, w| w.dacen().enabled()
                                   .tim2en().enabled());

        // configure PA4 and PA5 as analog inputs with no pull, so they don't
        // fight the DAC output
        gpioa.moder.modify(|_, w|
            w.moder4().analog()
             .moder5().analog()
        );
        gpioa.pupdr.modify(|_, w| unsafe {
            w.pupdr4().bits(0b00) // no pull
             .pupdr5().bits(0b00)
        });

        // configure TIM2 for triggering DAC/DMA
        tim2.cr1.modify(|_, w| unsafe {
            w.ckd().bits(0b00) // no sampling filter clock division
             .cms().bits(0b00) // not center-aligned mode
             .dir().bits(0)    // count up
            // Based on observed behavior (the DMA->DAC output seeming to hang), I'm speculating
            // that the ARPE bit needs to be set if the TIM2_ARR auto-reload register is going to
            // be updated on the fly, otherwise the timer can end up out in the weeds (i.e. TIM2_CNT
            // is bigger than TIM2_ARR, so nothing happens until the timer wraps).
             .arpe().bits(1)   // ARR register is buffered
        });
        tim2.cr2.modify(|_, w| unsafe { w.mms().bits(0b010) }); // trigger output: update event
        tim2.arr.write(|w| unsafe { w.bits(249) }); // 1kHz, can be changed by siggen_set_freq()
        tim2.psc.write(|w| unsafe { w.psc().bits(0) }); // prescaler of 1
        tim2.egr.write(|w| unsafe { w.ug().bits(1) }); // immediately update registers

        // configure DAC
        // - if we're not doing this just after reset:
        //     rcc.apb1rstr.modify(|_, w| unsafe { w.dacrst().bits(1) }); // set DAC reset bit
        //     rcc.apb1rstr.modify(|_, w| unsafe { w.dacrst().bits(0) }); // clear reset bit
        dac.cr.modify(|_, w| unsafe {
            // - channel2
            w.mamp2().bits(0b1011) // mask for noise/triangle mode is 4095
             .wave2().bits(0b00)   // wave generation disabled
             .tsel2().bits(0b100)  // trigger select TIM2 TRGO
             .ten2().bits(1)       // trigger enable
             .boff2().bits(0)      // output buffer enabled
            // - channel1
             .mamp1().bits(0b1011) // mask for noise/triangle mode is 4095
             .wave1().bits(0b00)   // wave generation disabled
             .tsel1().bits(0b100)  // trigger select TIM2 TRGO
             .ten1().bits(1)       // trigger enable
             .boff1().bits(0)      // output buffer enabled
        });

        // configure DMA2 channel 3 for DAC channel 2
        // - assuming reset state
        dma2.ccr3.modify(|_, w| unsafe {
            w.mem2mem().bits(0)  // memory-to-memory mode disabled
             .pl().bits(0b01)    // medium priority
             .msize().bits(0b01) // memory data size 16 bits
             .psize().bits(0b01) // peripheral data size 16 bits
             .minc().bits(1)     // memory increment enabled
             .pinc().bits(0)     // peripheral increment disabled
             .circ().bits(1)     // circular mode
             .dir().bits(1)      // transfer direction: memory -> peripheral
        });
        dma2.cndtr3.write(|w| unsafe { w.ndt().bits(144) });  // buffer size
        let dac_dhr12r2_address: u32 = &dac.dhr12r2 as *const _ as u32;
        debug_assert_eq!(dac_dhr12r2_address, 0x40007414);
        dma2.cpar3.write(|w| unsafe {
            w.bits(dac_dhr12r2_address) // peripheral base address
        });
        dma2.cmar3.write(|w| unsafe {
            w.bits(&SINE_12BIT as *const _ as u32) // memory base address
        });

        // configure DMA2 channel 4 for DAC channel 1
        // - assuming reset state
        dma2.ccr4.modify(|_, w| unsafe {
            w.mem2mem().bits(0)  // memory-to-memory mode disabled
             .pl().bits(0b01)    // medium priority
             .msize().bits(0b00) // memory data size 8 bits
             .psize().bits(0b00) // peripheral data size 8 bits
             .minc().bits(1)     // memory increment enabled
             .pinc().bits(0)     // peripheral increment disabled
             .circ().bits(1)     // circular mode
             .dir().bits(1)      // transfer direction: memory -> peripheral
        });
        dma2.cndtr4.write(|w| unsafe { w.ndt().bits(144) });  // buffer size
        let dac_dhr8r1_address: u32 = &dac.dhr8r1 as *const _ as u32;
        debug_assert_eq!(dac_dhr8r1_address, 0x40007410);
        dma2.cpar4.write(|w| unsafe {
            w.bits(dac_dhr8r1_address) // peripheral base address
        });
        dma2.cmar4.write(|w| unsafe {
            w.bits(&RAMP_8BIT as *const _ as u32) // memory base address
        });

        // enable DAC channels 1 and 2
        dac.cr.modify(|_, w| unsafe { w.en1().bits(1).en2().bits(1) });

        // enable DMA2 channels 3 and 4
        dma2.ccr3.modify(|_, w| unsafe { w.en().bits(1) });
        dma2.ccr4.modify(|_, w| unsafe { w.en().bits(1) });

        // enable DMA for DAC channels 1 and 2
        dac.cr.modify(|_, w| unsafe { w.dmaen1().bits(1).dmaen2().bits(1) });

        // enable TIM2  
        tim2.cr1.modify(|_, w| unsafe { w.cen().bits(1) });
    });
}

pub fn siggen_set_freq(freq: u32) {
    let arr = core::cmp::max(36_000_000 / 144 / freq - 1, 1);
    cortex_m::interrupt::free(|cs| {
        let tim2 = TIM2.borrow(cs);
        tim2.arr.write(|w| unsafe { w.bits(arr) });
    });
}