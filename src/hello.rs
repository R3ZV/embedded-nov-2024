//! Print a "Hello, World!" message to the debugger and blink the LED on GPIO1.

#![no_std]
#![no_main]
// Delete the following line after you're done implementing
// the solution.
#![allow(unused)]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::usb::{Driver, InterruptHandler};
use emb
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};
use log::Level;

// TODO 2.1 : Write a task that blinks the LED connected to GPIO1.
async fn blink(pin: AnyPin) {
   let mut led = Output::new(pin, Level::low, OutputDriver::Standard);

   loop {
       led.set_high();
       Timer::after_millis(150).await;
       led.set_low();
       Timer::after_millis(150).await;
   }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    spawner.spawn(blink(p.PIN_0.degrade())).unwrap();
    Timer::after_millis(5000).await;

    info!("Hello world!");
       Timer::after_millis(5000).await;
rustup component add rust-src
    // TODO 2.2 : Spawn the task that blinks the LED connected to GPIO1.
}
