#![no_std]
#![no_main]

use core::str::from_utf8;

use cyw43::{JoinOptions, PowerManagementMode};
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::pwm::{Config as PwmConfig, Pwm, PwmOutput};
use cyw43_firmware::CYW43_43439A0;
use cyw43_firmware::CYW43_43439A0_CLM;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::{info, unwrap, warn, Debug2Format};
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Config, StackResources};
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Level, Output};
use embedded_hal_1::pwm::SetDutyCycle;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::bind_interrupts;
use embassy_time::{Duration, Timer};
use embedded_io_async::Read;


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
const COMMAND_URL: &str = env!("IR_SUNRISE_URL");
const POLL_PERIOD_MS: u64 = 5_000;

const UP_PULSES: &[u32] = &[
    20_198, 6_833, 2_306, 6_873, 2_342, 11_348, 2_408, 6_818, 2_291, 6_873, 2_350,
];

const DOWN_PULSES: &[u32] = &[
    20_209, 6_795, 2_354, 6_794, 2_341, 11_418, 2_352, 9_127, 2_352, 2_213, 2_357,
];

const ON_PULSES: &[u32] = &[
    20_213, 6_845, 2_296, 6_870, 2_347, 9_166, 2_284, 4_641, 2_275, 2_287, 6_923, 16_010,
    20_701, 6_869, 2_340, 6_793, 2_352, 9_126, 2_347, 4_552, 2_272, 2_290, 6_882, 16_009,
    20_686, 6_856, 2_297, 6_887, 2_280, 9_185, 2_277, 4_543, 2_349, 2_291, 6_886,
];

const OFF_PULSES: &[u32] = &[
    20_195, 6_793, 2_375, 6_792, 2_350, 2_280, 9_202, 2_214, 2_358, 6_794, 2_362, 4_543,
    2_362, 11_396, 20_657, 6_875, 2_304, 6_802, 2_359, 2_221, 9_195, 2_285, 2_351, 6_814,
    2_361, 4_533, 2_283, 11_481, 20_636, 6_853, 2_354, 6_800, 2_348, 2_216, 9_282, 2_214,
    2_347, 6_817, 2_364, 4_478, 2_348,
];

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Command {
    Up,
    Down,
    On,
    Off,
}

impl Command {
    fn pulses(&self) -> &'static [u32] {
        match self {
            Command::Up => UP_PULSES,
            Command::Down => DOWN_PULSES,
            Command::On => ON_PULSES,
            Command::Off => OFF_PULSES,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Command::Up => "up",
            Command::Down => "down",
            Command::On => "on",
            Command::Off => "off",
        }
    }
}

#[derive(Deserialize)]
struct CommandPayload {
    command: Command,
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



async fn transmit_ir_signal(ir_led: &mut PwmOutput<'_>, pulses: &[u32]) {
    info!("Transmitting {} pulses with PWM carrier", pulses.len());

    if ir_led.set_duty_cycle_percent(0).is_err() {
        warn!("Failed to disable IR carrier");
    }

    let mut mark = true;
    for &duration_us in pulses {
        if mark {
            if ir_led.set_duty_cycle_percent(33).is_err() {
                warn!("Failed to enable IR carrier");
            }
        } else {
            if ir_led.set_duty_cycle_percent(0).is_err() {
                warn!("Failed to disable IR carrier");
            }
        }

        Timer::after(Duration::from_micros(duration_us as u64)).await;
        mark = !mark;
    }

    if ir_led.set_duty_cycle_percent(0).is_err() {
        warn!("Failed to disable IR carrier");
    }
    info!("IR transmission complete");
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Starting ir-sunrise polling binary");

    let p = embassy_rp::init(Default::default());
    let mut rng = RoscRng;

    let wifi_fw = CYW43_43439A0;
    let wifi_clm = CYW43_43439A0_CLM;
    let wifi_pwr = Output::new(p.PIN_23, Level::Low);
    let wifi_cs = Output::new(p.PIN_25, Level::High);
    let mut wifi_pio = Pio::new(p.PIO0, Irqs);
    let wifi_spi = PioSpi::new(
        &mut wifi_pio.common,
        wifi_pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        wifi_pio.irq0,
        wifi_cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, wifi_pwr, wifi_spi, wifi_fw).await;
    unwrap!(spawner.spawn(cyw43_task(runner)));

    control.init(wifi_clm).await;
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

    let poll_period = Duration::from_millis(POLL_PERIOD_MS);
    info!("Polling every {} ms", POLL_PERIOD_MS);
    info!("Polling URL: {}", COMMAND_URL);

    let carrier_top = clk_sys_freq() / 38_000 - 1;
    let mut ir_pwm_config = PwmConfig::default();
    ir_pwm_config.phase_correct = false;
    ir_pwm_config.invert_b = true;
    ir_pwm_config.top = carrier_top as u16;
    ir_pwm_config.compare_b = 0;
    info!("IR carrier PWM top: {}", carrier_top);

    let mut ir_pwm = Pwm::new_output_b(p.PWM_SLICE7, p.PIN_15, ir_pwm_config);
    let mut ir_led = ir_pwm.split_by_ref().1.unwrap();

    loop {
        if !stack.is_link_up() || !stack.is_config_up() {
            info!("Wi-Fi not connected; attempting reconnect");

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
        }

        info!("Polling IR command endpoint");
        let mut rx_buffer = [0u8; 4096];
        let client_state = TcpClientState::<1, 4096, 4096>::new();
        let tcp_client = TcpClient::new(stack, &client_state);
        let dns_client = DnsSocket::new(stack);
        let mut http_client = HttpClient::new(&tcp_client, &dns_client);

        let mut request = match http_client.request(Method::GET, COMMAND_URL).await {
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
                info!("Received command '{}'", payload.command.name());
                transmit_ir_signal(&mut ir_led, payload.command.pulses()).await;
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
