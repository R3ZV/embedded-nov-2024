//! Go to [Random Color Generator](https://randomwordgenerator.com/color.phpj)
//! Generate two colors and get the RGB encodings for them. These are the colors
//! you will need to display on the RGB LED.
//!
//! Your application should smoothly transition from one color to another. The colors will
//! be displayed sequentially for 3 seconds each, with a gradual transition period of 1 second.
//!
//! Keep in mind that the RGB LED is common anode.
//!
//! For displaying the color on the LED, PWM (Pulse Width Modulation) will need to be set up
//! on the pin. Connect them to pins: GPIO0 (Red), GPIO1 (Green), and
//! GPIO2 (Blue). (Hint: Pin 0 and 1 will share the same channel).

#![no_std]
#![no_main]
// Delete the following line after you're done implementing
// the solution.
#![allow(unused)]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Output, Pin};
use embassy_rp::peripherals::{PIN_0, PIN_1, PIN_2, PWM_SLICE0, PWM_SLICE1};
use embassy_rp::pwm::{Config as PwmConfig, Pwm, SetDutyCycle};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

fn get_min(a: u16, b: u16) -> u16 {
    if a > b {
        b
    } else {
        a
    }
}
fn blue_config(color: (u16, u16, u16)) -> PwmConfig {
    let mut config = PwmConfig::default();
    config.top = 255;
    config.compare_a = color.1;
    config.compare_a = color.2;

    config
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let color1 = (109, 63, 91);
    let color2 = (255, 164, 32);
    let mut color = color1;

    let mut config = PwmConfig::default();
    config.top = 255;
    config.compare_a = 109;
    config.compare_b = 63;

    let mut pwm_rg = Pwm::new_output_ab(p.PWM_SLICE0, p.PIN_0, p.PIN_1, config.clone());

    let mut pwm_b = Pwm::new_output_a(p.PWM_SLICE1, p.PIN_2, blue_config(color));

    loop {
        Timer::after_millis(50).await;
        pwm_rg.set_config(&config);
        pwm_b.set_config(&blue_config(color1));

        info!("r={}, g={}, b={}", color1.0, color1.1, color1.2);

        if color == color2 {
            color = color1;
        }

        color.0 = get_min(color.0 + 1, color2.0);
        color.1 = get_min(color.1 + 1, color2.1);
        color.2 = get_min(color.2 + 1, color2.2);

        config.compare_a = color.0;
        config.compare_b = color.1;
    }
}
