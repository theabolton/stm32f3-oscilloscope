// ST7735 LCD hardware configuration and low level functions

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

#![allow(non_snake_case)]

use core::ptr;

use cortex_m;
use stm32f30x::{GPIOB, RCC};
#[cfg(not(feature = "software-spi"))]
use stm32f30x::SPI2;

use parallax_8x12_font;
use { // C functions
    _st7735_drawFastHLine,
    _st7735_drawFastVLine,
    _st7735_drawPixel,
    _st7735_fillScreen,
    _st7735_get_height,
    _st7735_get_width,
    _st7735_initR,
    _st7735_pushColor,
    _st7735_setAddrWindow,
    _st7735_setRotation
};

// ======== ST7735 "type" and color enums ========

#[allow(unused)]
pub enum St7735Type {
    GreenTab = 0,
    RedTab,
    BlackTab,
}

#[allow(unused)]
#[derive(Clone,Copy)]
pub enum St7735Color {
    Black = 0,
    Blue = 0x001f,
    Green = 0x07e0,
    Red = 0xf800,
    White = 0xffff,
}

// ======== hardware SPI ========

// set up the hardware to use hardware SPI: SPI2 on PB13 (SCK/SCL) and PB15 (SDA/MOSI)
#[cfg(not(feature = "software-spi"))]
pub fn st7735_setup() {
    cortex_m::interrupt::free(|cs| {
        let rcc = RCC.borrow(cs);
        let gpiob = GPIOB.borrow(cs);
        let spi2 = SPI2.borrow(cs);
        rcc.ahbenr.modify(|_, w| w.iopben().enabled());
        rcc.apb1enr.modify(|_, w| w.spi2en().enabled());

        // configure GPIO pins
        gpiob.moder.modify(|_, w|
            w.moder10().output()    // PB10: CSE/CS
             .moder12().output()    // PB12: A0/RS/DC
             .moder13().alternate() // PB13: SCK/SCL
             .moder14().output()    // PB14: RST
             .moder15().alternate() // PB15: SDA/MOSI
        );
        gpiob.otyper.modify(|_, w| unsafe {
            w.ot10().bits(0) // push-pull
             .ot12().bits(0)
             .ot13().bits(0)
             .ot14().bits(0)
             .ot15().bits(0)
        });
        gpiob.ospeedr.modify(|_, w| unsafe {
            w.ospeedr10().bits(0b11) // fast
             .ospeedr12().bits(0b11)
             .ospeedr13().bits(0b11)
             .ospeedr14().bits(0b11)
             .ospeedr15().bits(0b11)
        });
        // set alternate function on SPI2 CLK and MOSI pins
        gpiob.afrh.modify(|_, w| unsafe {
            w.afrh13().bits(0b0101) // PB13 AF5: SPI2_CLK - SCK/SCL
             .afrh15().bits(0b0101) // PB15 AF5: SPI2_MOSI - SDA
        });

        // configure SPI2
        rcc.apb1rstr.modify(|_, w| unsafe { w.spi2rst().bits(1) }); // set SPI2 reset bit
        rcc.apb1rstr.modify(|_, w| unsafe { w.spi2rst().bits(0) }); // clear reset bit
        spi2.cr1.write(|w| unsafe {
            w.bidimode().bits(1)
             .bidioe().bits(0)   // will change to 1 after enable
             .ssm().bits(1)      // SPI_NSS_Soft
             .ssi().bits(1)      // part of 'SPI_Mode_Master'! will mode fault without this!
             .lsbfirst().bits(0) // SPI_FirstBit_MSB
             .br().bits(0b010)   // f_PCLK/8 - 72MHz/2/8 = 4.5MHz, or near the ST7735's limit
             .mstr().bits(1)     // SPI_Mode_Master
             .cpol().bits(0)     // SPI_CPOL_Low
             .cpha().bits(0)     // SPI_CPHA_1Edge
        });
        spi2.cr2.modify(|_, w| unsafe { w.ds().bits(0b0111) }); // SPI_DataSize_8b
        spi2.i2scfgr.modify(|_, w| unsafe { w.i2smod().bits(0) }); // SPI mode
        // enable SPI2
        spi2.cr1.modify(|_, w| unsafe { w.spe().bits(1) });
        // set direction to transmit
        spi2.cr1.modify(|_, w| unsafe { w.bidioe().bits(1) });
    });
}

// send a byte of data to the LCD via hardware SPI
#[cfg(not(feature = "software-spi"))]
fn st7735_send_byte(data_in: u8) {
    unsafe {
        while (*SPI2.get()).sr.read().txe().bits() == 0 {}
        // This is what I first naïvely tried:
        //   (*SPI2.get()).dr.write(|w| w.bits(data_in as u32));
        // And then I tried this:
        //   (*SPI2.get()).dr.write(|w| w.dr().bits(data_in as u16));
        // Both cause a double write to the Tx FIFO.
        // See RM0316, p.965, "Data Packing" and Figure 356.

        // This works, because it is an 8-bit write:
        ptr::write_volatile(&(*SPI2.get()).dr as *const _ as *mut u8, data_in);
    }
}

#[cfg(not(feature = "software-spi"))]
fn spi2_wait_while_busy() {
    unsafe {
        while (*SPI2.get()).sr.read().bsy().bits() != 0 {}
    }
}

#[cfg(not(feature = "software-spi"))]
fn lcd_dc() -> bool {
    0 != unsafe { (*GPIOB.get()).odr.read().odr12().bits() } // read PB12: A0/RS/DC
}

// send a command byte to the LCD controller
#[cfg(not(feature = "software-spi"))]
#[no_mangle]
#[used]
pub extern "C" fn st7735_send_cmd(cmd: u8) {
    if lcd_dc() {
        // drain the transmit FIFO before switching A0/DC
        spi2_wait_while_busy();
        lcd_dc0();
    }
    st7735_send_byte(cmd);
}

// send a data byte to the LCD controller
#[cfg(not(feature = "software-spi"))]
#[no_mangle]
#[used]
pub extern "C" fn st7735_send_data(data: u8) {
    if !lcd_dc() {
        // drain the transmit FIFO before switching A0/DC
        spi2_wait_while_busy();
        lcd_dc1();
    }
    st7735_send_byte(data);
}

// ======== software SPI ========

// set up the hardware to use software SPI: bit-banging on PB13 (SCK/SCL) and PB15 (SDA/MOSI)
#[cfg(feature = "software-spi")]
pub fn st7735_setup() {
    cortex_m::interrupt::free(|cs| {
        let rcc = RCC.borrow(cs);
        let gpiob = GPIOB.borrow(cs);
        rcc.ahbenr.modify(|_, w| w.iopben().enabled());

        lcd_sck0(); // set PB13 (SCK/SCL) to idle (low)

        gpiob.moder.modify(|_, w|
            w.moder10().output() // PB10: CSE/CS
             .moder12().output() // PB12: A0/RS/DC
             .moder13().output() // PB13: SCK/SCL
             .moder14().output() // PB14: RST
             .moder15().output() // PB15: SDA/MOSI
        );
        gpiob.otyper.modify(|_, w| unsafe {
            w.ot10().bits(0) // push-pull
             .ot12().bits(0)
             .ot13().bits(0)
             .ot14().bits(0)
             .ot15().bits(0)
        });
        gpiob.ospeedr.modify(|_, w| unsafe {
            w.ospeedr10().bits(0b11) // fast
             .ospeedr12().bits(0b11)
             .ospeedr13().bits(0b11)
             .ospeedr14().bits(0b11)
             .ospeedr15().bits(0b11)
        });
    });
}

// send a byte of data to the LCD controller via bit-banged SPI
#[cfg(feature = "software-spi")]
fn st7735_send_byte(data_in: u8) {
    let mut data = data_in;
    for _ in 0..8 {
        if (data & 0x80) != 0 {
            unsafe { (*GPIOB.get()).bsrr.write(|w| w.bs15().set()); } // set PB15: SDA/MOSI
        } else {
            unsafe { (*GPIOB.get()).brr.write(|w| w.br15().bits(1)); } // reset PB15: SDA/MOSI
        }
        data = data << 1;
        // output clock pulse
        lcd_sck1();
        lcd_sck0();
    }
}

#[cfg(feature = "software-spi")]
fn lcd_sck1() {
    unsafe { (*GPIOB.get()).bsrr.write(|w| w.bs13().set()); } // set PB13: SCK/SCL
}

#[cfg(feature = "software-spi")]
fn lcd_sck0() {
    unsafe { (*GPIOB.get()).brr.write(|w| w.br13().bits(1)); } // reset PB13: SCK/SCL
}

// send a command byte to the LCD controller
#[cfg(feature = "software-spi")]
#[no_mangle]
#[used]
pub extern "C" fn st7735_send_cmd(cmd: u8) {
    lcd_dc0();
    st7735_send_byte(cmd);
}

// send a data byte to the LCD controller
#[cfg(feature = "software-spi")]
#[no_mangle]
#[used]
pub extern "C" fn st7735_send_data(data: u8) {
    lcd_dc1();
    st7735_send_byte(data);
}

// ======== SPI/GPIO manipulation functions for both hardware and software modes ========

#[no_mangle]
#[used]
pub extern "C" fn lcd_cs1() {
    unsafe { (*GPIOB.get()).bsrr.write(|w| w.bs10().set()); } // set PB10: CSE/CS
}

#[no_mangle]
#[used]
pub extern "C" fn lcd_cs0() {
    unsafe { (*GPIOB.get()).brr.write(|w| w.br10().bits(1)); } // reset PB10: CSE/CS
}

fn lcd_dc1() {
    unsafe { (*GPIOB.get()).bsrr.write(|w| w.bs12().set()); } // set PB12: A0/RS/DC
}

fn lcd_dc0() {
    unsafe { (*GPIOB.get()).brr.write(|w| w.br12().bits(1)); } // reset PB12: A0/RS/DC
}

#[no_mangle]
#[used]
pub extern "C" fn lcd_rst1() {
    unsafe { (*GPIOB.get()).bsrr.write(|w| w.bs14().set()); } // set PB14: RST
}

#[no_mangle]
#[used]
pub extern "C" fn lcd_rst0() {
    unsafe { (*GPIOB.get()).brr.write(|w| w.br14().bits(1)); } // reset PB14: RST
}

// ======== wrappers for (unsafe) C functions ========

pub fn st7735_initR(lcd_type: u8) { unsafe { _st7735_initR(lcd_type) } }

#[allow(unused)]
pub fn st7735_drawFastHLine(x: i16, y: i16, w: i16, color: u16) {
    unsafe { _st7735_drawFastHLine(x, y, w, color) }
}

#[allow(unused)]
pub fn st7735_drawFastVLine(x: i16, y: i16, h: i16, color: u16) {
    unsafe { _st7735_drawFastVLine(x, y, h, color) }
}

pub fn st7735_drawPixel(x: i16, y: i16, color: u16) { unsafe { _st7735_drawPixel(x, y, color) } }

pub fn st7735_fillScreen(color: u16) { unsafe { _st7735_fillScreen(color) } }

pub fn st7735_pushColor(color: u16) { unsafe { _st7735_pushColor(color) } }

pub fn st7735_setAddrWindow(x0: u8, y0: u8, x1: u8, y1: u8) {
    unsafe { _st7735_setAddrWindow(x0, y0, x1, y1) }
}

pub fn st7735_setRotation(rotation: u8) { unsafe { _st7735_setRotation(rotation) } }

pub fn st7735_get_height() -> u8 { unsafe { _st7735_get_height() } }

pub fn st7735_get_width() -> u8 { unsafe { _st7735_get_width() } }

// ======== text printing ========

fn st7735_putc_unchecked(x: u8, y:u8, c: u8, fg: St7735Color, bg: St7735Color) {
    if c >= 128 {
        return;
    }
    st7735_setAddrWindow(x, y, x + 7, y + 11);
    for yrow in 0..12 {
        let mut bits = parallax_8x12_font::FONT_8X12[(c as usize) * 12 + yrow];
        for _ in 0..8 {
            if bits & 0b1 == 0b1 {
                st7735_pushColor(fg as u16);
            } else {
                st7735_pushColor(bg as u16);
            }
            bits >>= 1;
        }
    }
}

#[allow(unused)]
pub fn st7735_putc(x: u8, y:u8, c: u8, fg: St7735Color, bg: St7735Color) {
    let height = st7735_get_height();
    let width = st7735_get_width();
    if x > width - 8 || y > height - 12 {
        return;
    }
    st7735_putc_unchecked(x, y, c, fg, bg);
}

#[allow(unused)]
pub fn st7735_print(x0: u8, y: u8, text: &[u8], fg: St7735Color, bg: St7735Color) {
    let height = unsafe { st7735_get_height() };
    let width = unsafe { st7735_get_width() };
    let mut x = x0;
    if y > height - 12 {
        return;
    }
    for c in text {
        if x > width - 8 {
            return;
        }
        st7735_putc_unchecked(x, y, *c, fg, bg);
        x += 8;
    }
}
