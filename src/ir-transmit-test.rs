#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::pwm::{Config as PwmConfig, Pwm, PwmOutput};
use embassy_time::{Duration, Timer};
use embedded_hal_1::pwm::SetDutyCycle;
use {defmt_rtt as _, panic_probe as _};

// const ON_PULSES: &[u32] = &[
//     20_213, 6_845, 2_296, 6_870, 2_347, 9_166, 2_284, 4_641, 2_275, 2_287, 6_923, 16_010,
//     20_701, 6_869, 2_340, 6_793, 2_352, 9_126, 2_347, 4_552, 2_272, 2_290, 6_882, 16_009,
//     20_686, 6_856, 2_297, 6_887, 2_280, 9_185, 2_277, 4_543, 2_349, 2_291, 6_886,
// ];
const ON_PULSES: &[u32] = &[
    20_650, 6_850, 2_320, 6_850, 2_320, 11_430, 2_320, 6_850, 2_320, 6_850, 2_320,
];

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
        } else if ir_led.set_duty_cycle_percent(0).is_err() {
            warn!("Failed to disable IR carrier");
        }

        Timer::after(Duration::from_micros(duration_us as u64)).await;
        mark = !mark;
    }

    if ir_led.set_duty_cycle_percent(0).is_err() {
        warn!("Failed to disable IR carrier");
    }

    info!("IR transmission complete");
}

async fn ir_transmit_loop(ir_led: &mut PwmOutput<'_>) -> ! {
    loop {
        info!("Sending command 'on'");
        transmit_ir_signal(ir_led, ON_PULSES).await;
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    info!("IR transmit test starting");

    let carrier_top = clk_sys_freq() / 38_000 - 1;
    let mut ir_pwm_config = PwmConfig::default();
    ir_pwm_config.phase_correct = false;
    ir_pwm_config.invert_b = true;
    ir_pwm_config.top = carrier_top as u16;
    ir_pwm_config.compare_b = 0;

    info!("IR carrier PWM top: {}", carrier_top);

    let mut ir_pwm = Pwm::new_output_b(p.PWM_SLICE7, p.PIN_15, ir_pwm_config);
    let mut ir_led = ir_pwm.split_by_ref().1.unwrap();

    ir_transmit_loop(&mut ir_led).await;
}
