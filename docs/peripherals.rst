+++++++++++++++++++++++++++++++++++++++
STM32F3 Oscilloscope - Peripheral Usage
+++++++++++++++++++++++++++++++++++++++

As best I understand it, the features that allow the svd2rust-generated device definition
crates to provide the memory safety and register-legal-value safety that they do, are the very same features that make it
difficult to write the sort of generic-access facilities that a HAL would provide. For now,
I am just hard-coding the specific peripherals used, and will work on abstracting their use
later. For reference, here is the list of peripherals used by all modules of
stm32f3-oscilloscope:

::

   Pushbuttons
      PD12 - pushbutton 1 (left, timebase)
      PD13 - pushbutton 2
      PD14 - pushbutton 3
      PD15 - pushbutton 4 (right, siggen frequency)
   LEDs
      PE8 / LD4  - (NW, blue) initialization completed indicator
      PE10 / LD5 - (NE, orange) toggle after each display sweep
   ST7735 LCD Display
      SPI2
      PB10 - CSE/CS
      PB12 - A0/RS/DC
      PB13 - SPI2 SCK/SCL
      PB14 - RST
      PB15 - SPI2 SDA/MOSI
   Capture
      ADC1
      PC1  - input GPIO
      TIM15
   Signal Generator
      DAC1 channels 1 and 2
      DMA2 channels 3 and 4
      TIM2
      PA4 - "sine" wave output
      PA5 - "ramp" (escalator) output
   System Clocks
      HSE external 8MHz clock from on-board ST-Link
      PLL set for 9 multiplier
      HCLK/AHB at 72Mhz
      APB2 at 72MHz
      APB1 at 36MHz
      FLASH set to 2 wait states
      SysTick update exception every 1ms
