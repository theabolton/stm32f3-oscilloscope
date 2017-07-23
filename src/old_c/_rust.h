// functions defined in Rust
#ifndef _RUST_H
#define _RUST_H

#include <stdlib.h>
#include <stdint.h>

extern void delay_ms(unsigned int);
extern void st7735_send_cmd(const uint8_t cmd);
extern void st7735_send_data(const uint8_t data);
extern void st7735_fill_rect(int16_t x, int16_t y, int16_t w, int16_t h, uint16_t color);
extern void lcd_cs0(void);
extern void lcd_cs1(void);
extern void lcd_rst1(void);
extern void lcd_rst0(void);

#endif /* _RUST_H */
