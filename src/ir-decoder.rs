#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::uart::{Config as UartConfig, UartTx};
use embassy_time::{Duration, Instant, Timer};
use ir_sunrise::IrSignal;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    
    info!("IR Sunrise - Starting");

    // Configure UART0 for serial communication
    // TX on GPIO 0 (Pin 1), RX on GPIO 1 (Pin 2) - standard UART0 pins
    let mut uart_config = UartConfig::default();
    uart_config.baudrate = 115200;
    
    let mut uart = UartTx::new_blocking(p.UART0, p.PIN_0, uart_config);

    info!("UART Serial ready on GPIO 0 (TX) at 115200 baud");

    // IR Receiver connected to GPIO 16 (Pin 21 on RP2350)
    // This pin can be changed based on your wiring
    let mut ir_pin = Input::new(p.PIN_16, Pull::None);

    info!("Starting IR receiver on GPIO 16");
    
    let timeout_us = 50_000u64; // 50ms timeout to detect end of signal
    
    loop {
        let mut signal = IrSignal::new();
        
        // Wait for initial signal (LOW, since TSOP38238 outputs LOW when IR detected)
        info!("Waiting for IR signal...");
        ir_pin.wait_for_low().await;
        
        info!("IR signal detected, recording...");
        
        let mut last_state = false; // LOW state
        let mut pulse_start = Instant::now();
        
        // Capture the signal
        loop {
            let current_state = ir_pin.is_high();
            
            if current_state != last_state {
                let pulse_end = Instant::now();
                let duration = pulse_end.duration_since(pulse_start);
                let duration_us = duration.as_micros() as u32;
                
                if duration_us > 10 { // Ignore glitches < 10us
                    if signal.add_pulse(duration_us).is_err() {
                        error!("Signal buffer full!");
                        break;
                    }
                }
                
                pulse_start = pulse_end;
                last_state = current_state;
            }
            
            // Check for timeout (no state change means signal ended)
            if Instant::now().duration_since(pulse_start).as_micros() > timeout_us {
                signal.complete = true;
                break;
            }
            
            // Small yield to prevent busy-waiting
            Timer::after(Duration::from_micros(10)).await;
        }
        
        if signal.complete && !signal.pulses.is_empty() {
            info!("Signal captured: {} pulses", signal.pulses.len());
            
            // Convert to text format
            let text = signal.to_text();
            info!("Sending via UART: {} bytes", text.len());
            
            // Send over UART serial
            uart.blocking_write(text.as_bytes()).unwrap();
            info!("Signal sent successfully");
        } else {
            warn!("Signal incomplete or empty");
        }
        
        // Small delay before next capture
        Timer::after(Duration::from_millis(500)).await;
    }
}
