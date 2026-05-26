#![no_std]
#![no_main]

use core::fmt::Write as _;
use core::str::from_utf8;

use cyw43::{JoinOptions, PowerManagementMode};
use cyw43_firmware::CYW43_43439A0;
use cyw43_firmware::CYW43_43439A0_CLM;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::{error, info, unwrap, warn, Debug2Format};
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Config, StackResources};
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::bind_interrupts;
use embassy_time::{Duration, Timer};
use embedded_io_async::Read;
use heapless::String;
use ir_sunrise::IrSignal;
use reqwless::client::HttpClient;
use reqwless::request::Method;
use serde::Deserialize;
use serde_json_core::from_slice;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

const WIFI_SSID: &str = env!("IR_SUNRISE_WIFI_SSID");
const WIFI_PASSWORD: &str = env!("IR_SUNRISE_WIFI_PASSWORD");
const HOSTNAME: &str = env!("IR_SUNRISE_HOST");

#[derive(Deserialize)]
struct CommandPayload<'a> {
    #[serde(default, borrow)]
    command: Option<&'a str>,
}

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, embassy_rp::peripherals::DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

fn poll_period() -> Duration {
    if let Some(ms_str) = option_env!("IR_SUNRISE_PERIOD_MS") {
        if let Ok(ms) = ms_str.parse::<u64>() {
            return Duration::from_millis(ms);
        }
        warn!("Failed to parse IR_SUNRISE_PERIOD_MS='{}', defaulting to 5000ms", ms_str);
    } else {
        warn!("IR_SUNRISE_PERIOD_MS not set, defaulting to 5000ms");
    }
    Duration::from_millis(5000)
}

fn url_path() -> &'static str {
    option_env!("IR_SUNRISE_PATH").unwrap_or("/")
}

fn build_url() -> Option<String<128>> {
    let mut url = String::<128>::new();
    if write!(&mut url, "http://{}{}", HOSTNAME, url_path()).is_err() {
        return None;
    }
    Some(url)
}

async fn transmit_ir_signal(led: &mut Output<'_>, pulses: &[u32]) {
    info!("Transmitting {} pulses", pulses.len());

    led.set_high();
    let mut is_low = true;

    for &duration_us in pulses {
        if is_low {
            led.set_low();
        } else {
            led.set_high();
        }

        Timer::after(Duration::from_micros(duration_us as u64)).await;
        is_low = !is_low;
    }

    led.set_high();
    info!("IR transmission complete");
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Starting ir-sunrise polling binary");

    let p = embassy_rp::init(Default::default());
    let mut rng = RoscRng;

    let fw = CYW43_43439A0;
    let clm = CYW43_43439A0_CLM;

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    unwrap!(spawner.spawn(cyw43_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(PowerManagementMode::PowerSave)
        .await;

    let config = Config::dhcpv4(Default::default());
    static NET_RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    let seed = rng.next_u64();
    let (stack, net_runner) = embassy_net::new(
        net_device,
        config,
        NET_RESOURCES.init(StackResources::new()),
        seed,
    );

    unwrap!(spawner.spawn(net_task(net_runner)));

    while let Err(err) = control
        .join(WIFI_SSID, JoinOptions::new(WIFI_PASSWORD.as_bytes()))
        .await
    {
        warn!("Wi-Fi join failed: {}", Debug2Format(&err));
        Timer::after(Duration::from_secs(2)).await;
    }

    info!("Waiting for link up...");
    stack.wait_link_up().await;

    info!("Waiting for DHCP configuration...");
    stack.wait_config_up().await;
    info!("Network stack is ready");

    let poll_period = poll_period();
    info!("Polling every {} ms", poll_period.as_millis());

    let command_url = match build_url() {
        Some(url) => url,
        None => {
            error!("Failed to build polling URL, halting");
            loop {
                Timer::after(Duration::from_secs(10)).await;
            }
        }
    };
    info!("Polling URL: {}", command_url.as_str());

    let mut ir_led = Output::new(p.PIN_15, Level::High);

    loop {
        info!("Polling IR command endpoint");
        let mut rx_buffer = [0u8; 4096];
        let client_state = TcpClientState::<1, 4096, 4096>::new();
        let tcp_client = TcpClient::new(stack, &client_state);
        let dns_client = DnsSocket::new(stack);
        let mut http_client = HttpClient::new(&tcp_client, &dns_client);

        let mut request = match http_client.request(Method::GET, command_url.as_str()).await {
            Ok(req) => req,
            Err(err) => {
                warn!("HTTP request creation failed: {:?}", err);
                Timer::after(poll_period).await;
                continue;
            }
        };

        let response = match request.send(&mut rx_buffer).await {
            Ok(resp) => resp,
            Err(err) => {
                warn!("HTTP request send failed: {:?}", err);
                Timer::after(poll_period).await;
                continue;
            }
        };

        if response.status.0 != 200 {
            warn!("Unexpected HTTP status {}", response.status.0);
            Timer::after(poll_period).await;
            continue;
        }

        let mut body_buffer = [0u8; 2048];
        let body_bytes = match response.body().reader().read(&mut body_buffer).await {
            Ok(n) => &body_buffer[..n],
            Err(err) => {
                warn!("Failed to read response body: {:?}", err);
                Timer::after(poll_period).await;
                continue;
            }
        };

        if body_bytes.is_empty() {
            info!("No content in response");
            Timer::after(poll_period).await;
            continue;
        }

        let body_str = match from_utf8(body_bytes) {
            Ok(text) => text,
            Err(err) => {
                warn!("Response not valid UTF-8: {}", Debug2Format(&err));
                Timer::after(poll_period).await;
                continue;
            }
        };

        match from_slice::<CommandPayload>(body_str.as_bytes()) {
            Ok((payload, _)) => {
                if let Some(command_str) = payload.command.map(|c| c.trim()) {
                    if command_str.is_empty() {
                        info!("Received empty command");
                    } else {
                        info!("Received command '{}'", command_str);

                        match IrSignal::from_text(command_str) {
                            Some(signal) => {
                                transmit_ir_signal(&mut ir_led, signal.pulses.as_slice()).await;
                            }
                            None => {
                                warn!("Failed to parse IR command payload");
                            }
                        }
                    }
                } else {
                    info!("No command present in response");
                }
            }
            Err(err) => {
                warn!(
                    "Failed to parse JSON payload: {}",
                    Debug2Format(&err)
                );
                let preview = if body_str.len() > 64 {
                    &body_str[..64]
                } else {
                    body_str
                };
                info!("Response preview: {}", preview);
            }
        }

        Timer::after(poll_period).await;
    }
}