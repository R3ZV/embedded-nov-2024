//! Your Pico should act as the core MCU of a smart clock system.
//! Your device should have the following functionalities:
//!     * It will display the current time and temperature in the room.
//! In order to do that it will send a get request to the provided ip and port
//! to get the current time at the beginning of the runtime.
//!     * It will have provide a visual feedback of the current temperature
//! by displaying a color between red and blue on the RGB led, depending on
//! configurable maximum and minimum thresholds.
//!     * In order to update the thresholds, the desired behaviour is to
//! enter the configure mode by pressing the A button, then the current minimum
//! threshold value will be displayed on the screen, and by pressing X and Y,
//! the user should be able to increase and decrease respectively be half a
//! degree, then confirm it by pressing A once again, and proceed to setting
//! the maximum threshold in the same fashion.
//!     * To ensure redundency, the thresholds will be written in the provided
//! EEPROM24C256 when set, and read at the beginning of the program.
//!     * BONUS: We will simulate the fact that the clock is part of an evil
//! IoT network that spies on its users by sending a JSON package via HTTPS
//! to the same server, containing the datetime and the temperature.

#![no_std]
#![no_main]

use core::str::from_utf8;
use cyw43::JoinOptions;
use cyw43_pio::PioSpi;
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Ipv4Address, Ipv4Cidr, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_time::{Duration, Timer};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Point;
use embedded_graphics::mono_font::iso_8859_1::FONT_7X13_BOLD;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::text::Text;
use embedded_graphics::Drawable;
use embedded_nov_2024::display::SPIDeviceInterface;
use heapless::Vec;
use reqwless::client::{HttpClient, TlsConfig, TlsVerify};
use reqwless::request::Method;
use serde::Deserialize;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _, serde_json_core};

const DISPLAY_FREQ: u32 = 64_000_000;

const WIFI_NETWORK: &str = "Wyeiodrin";
const WIFI_PASSWORD: &str = "g3E2PjWy";

#[embassy_executor::task]
async fn cyw43_task(runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[derive(Deserialize)]
struct ApiDate {
    year: u16,
    month: u16,
    day: u16,
}

#[derive(Deserialize)]
struct ApiTime {
    hour: u16,
    minite: u16,
    second: u16
}

#[derive(Deserialize)]
struct ApiResponse {
    time: ApiTime,
    date: ApiDate,
}

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let peripherals = embassy_rp::init(Default::default());

    info!("Initializing display...");

    // ************** Display initialization - DO NOT MODIFY! *****************
    let miso = peripherals.PIN_4;
    let display_cs = peripherals.PIN_17;
    let mosi = peripherals.PIN_19;
    let clk = peripherals.PIN_18;
    let rst = peripherals.PIN_0;
    let dc = peripherals.PIN_16;
    let mut display_config = embassy_rp::spi::Config::default();
    display_config.frequency = DISPLAY_FREQ;
    display_config.phase = embassy_rp::spi::Phase::CaptureOnSecondTransition;
    display_config.polarity = embassy_rp::spi::Polarity::IdleHigh;

    // Init SPI
    let spi: embassy_rp::spi::Spi<'_, _, embassy_rp::spi::Blocking> =
        embassy_rp::spi::Spi::new_blocking(
            peripherals.SPI0,
            clk,
            mosi,
            miso,
            display_config.clone(),
        );
    let spi_bus: embassy_sync::blocking_mutex::Mutex<
        embassy_sync::blocking_mutex::raw::NoopRawMutex,
        _,
    > = embassy_sync::blocking_mutex::Mutex::new(core::cell::RefCell::new(spi));

    let display_spi = embassy_embedded_hal::shared_bus::blocking::spi::SpiDeviceWithConfig::new(
        &spi_bus,
        embassy_rp::gpio::Output::new(display_cs, embassy_rp::gpio::Level::High),
        display_config,
    );

    let dc = embassy_rp::gpio::Output::new(dc, embassy_rp::gpio::Level::Low);
    let rst = embassy_rp::gpio::Output::new(rst, embassy_rp::gpio::Level::Low);
    let di = SPIDeviceInterface::new(display_spi, dc);

    // Init ST7789 LCD
    let mut display = st7789::ST7789::new(di, rst, 240, 240);
    display.init(&mut embassy_time::Delay).unwrap();
    display
        .set_orientation(st7789::Orientation::Portrait)
        .unwrap();
    display.clear(<embedded_graphics::pixelcolor::Rgb565 as embedded_graphics::pixelcolor::RgbColor>::BLACK).unwrap();
    // ************************************************************************

    info!("Display initialization finished!");

    let fw = unsafe { core::slice::from_raw_parts(0x10100000 as *const u8, 230321) };
    let clm = unsafe { core::slice::from_raw_parts(0x10140000 as *const u8, 4752) };

    let pwr = Output::new(peripherals.PIN_23, Level::Low);
    let cs = Output::new(peripherals.PIN_25, Level::High);
    let mut pio = Pio::new(peripherals.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        pio.irq0,
        cs,
        peripherals.PIN_24,
        peripherals.PIN_29,
        peripherals.DMA_CH0,
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    spawner.spawn(cyw43_task(runner));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    let config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 1, 9), 24),
        dns_servers: Vec::new(),
        gateway: Some(Ipv4Address::new(192, 168, 1, 1)),
    });

    // Generate random seed
    let seed = 69;

    // Init network stack
    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner.spawn(net_task(runner));

    loop {
        match control
            .join(WIFI_NETWORK, JoinOptions::new(WIFI_PASSWORD.as_bytes()))
            .await
        {
            Ok(_) => break,
            Err(err) => {
                info!("join failed with status={}", err.status);
            }
        }
    }

    // Wait for DHCP, not necessary when using static IP
    info!("waiting for DHCP...");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }
    info!("DHCP is now up!");

    info!("waiting for link up...");
    while !stack.is_link_up() {
        Timer::after_millis(500).await;
    }
    info!("Link is up!");

    info!("waiting for stack to be up...");
    stack.wait_config_up().await;
    info!("Stack is up!");
    let mut rx_buffer = [0; 8192];
    let mut tls_read_buffer = [0; 16640];
    let mut tls_write_buffer = [0; 16640];

    let client_state = TcpClientState::<1, 1024, 1024>::new();
    let tcp_client = TcpClient::new(stack, &client_state);
    let dns_client = DnsSocket::new(stack);
    let tls_config = TlsConfig::new(
        seed,
        &mut tls_read_buffer,
        &mut tls_write_buffer,
        TlsVerify::None,
    );

    let mut http_client = HttpClient::new(&tcp_client, &dns_client);
    let url = "http://192.168.1.199:5000/time";

    info!("connecting to {}", &url);

    let mut request = match http_client.request(Method::GET, &url).await {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to make HTTP request: {:?}", e);
            return; // handle the error
        }
    };

    let response = match request.send(&mut rx_buffer).await {
        Ok(resp) => resp,
        Err(_e) => {
            error!("Failed to send HTTP request");
            return; // handle the error;
        }
    };

    let body = match from_utf8(response.body().read_to_end().await.unwrap()) {
        Ok(b) => b,
        Err(_e) => {
            error!("Failed to read response body");
            return; // handle the error
        }
    };
    info!("Response body: {:?}", &body);

    // parse the response body and update the RTC

    let bytes = body.as_bytes();
    match serde_json_core::de::from_slice::<ApiResponse>(bytes) {
        Ok((output, _used)) => {
            info!("Datetime: {:?}", output.date.day);
        }
        Err(_e) => {
            error!("Failed to parse response body");
            return; // handle the error
        }
    }

    Timer::after(Duration::from_secs(5)).await;

    // Write welcome message
    let style = MonoTextStyle::new(&FONT_7X13_BOLD, Rgb565::CYAN);
    Text::new("Welcome to Rust Workshop!", Point::new(36, 190), style)
        .draw(&mut display)
        .unwrap();

    // Wait a bit
    Timer::after_secs(10).await;

    // Clear display
    display.clear(Rgb565::BLACK).unwrap();
    loop {
        Timer::after_secs(1).await;
    }
}
