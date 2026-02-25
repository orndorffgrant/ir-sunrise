#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::UART0;
use embassy_rp::uart::{BufferedInterruptHandler, BufferedUart, Config as UartConfig};
use embassy_rp::bind_interrupts;
use embassy_time::{Duration, Timer};
use embedded_io_async::Read;
use heapless::String;
use ir_sunrise::IrSignal;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
});

// Hardcoded IR signals - Add your captured signals here
// Format: "SIGNAL_NAME:pulse_count:duration1,duration2,..."
static IR_SIGNALS: &[&str] = &[
    // Example NEC protocol power button (replace with your actual captures)
    "POWER:67:9000,4500,560,560,560,1690,560,560,560,1690,560,560,560,560,560,560,560,560,560,1690,560,1690,560,560,560,1690,560,560,560,1690,560,1690,560,1690,560,560,560,560,560,560,560,1690,560,560,560,560,560,560,560,560,560,1690,560,1690,560,1690,560,560,560,1690,560,1690,560,1690,560,1690,560",
    
    // Add more signals here - format: "NAME:count:pulse1,pulse2,..."
    "VOLUP:67:9000,4500,560,560,560,1690,560,560,560,1690,560,560,560,560,560,560,560,560,560,1690,560,1690,560,560,560,1690,560,560,560,1690,560,1690,560,1690,560,560,560,1690,560,560,560,1690,560,560,560,560,560,560,560,560,560,1690,560,560,560,1690,560,560,560,1690,560,1690,560,1690,560,1690,560",
    
    "VOLDOWN:67:9000,4500,560,560,560,1690,560,560,560,1690,560,560,560,560,560,560,560,560,560,1690,560,1690,560,560,560,1690,560,560,560,1690,560,1690,560,1690,560,1690,560,1690,560,560,560,1690,560,560,560,560,560,560,560,560,560,560,560,560,560,1690,560,560,560,1690,560,1690,560,1690,560,1690,560",
];

// Maximum line buffer size
const MAX_LINE_LEN: usize = 256;

// Transmit IR signal on the LED
async fn transmit_ir_signal(led: &mut Output<'_>, pulses: &[u32]) {
    info!("Transmitting {} pulses", pulses.len());
    
    // Start with LED off (HIGH = off for IR LED in common configurations)
    led.set_low();
    
    let mut is_low = true;
    
    for &duration_us in pulses {
        if is_low {
            led.set_low(); // IR LED on
        } else {
            led.set_high(); // IR LED off
        }
        
        Timer::after(Duration::from_micros(duration_us as u64)).await;
        is_low = !is_low;
    }
    
    // Ensure LED is off at the end
    led.set_high();
    
    info!("Transmission complete");
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    
    info!("IR Transmitter - Starting");

    // Configure UART0 for serial communication
    let mut uart_config = UartConfig::default();
    uart_config.baudrate = 115200;
    
    // UART RX/TX buffers - must have 'static lifetime for BufferedUart
    // SAFETY: These buffers are only accessed through the UART peripheral.
    // main() is only called once, so these mutable references are never aliased.
    static mut TX_BUF: [u8; 256] = [0u8; 256];
    static mut RX_BUF: [u8; 256] = [0u8; 256];
    
    let uart = BufferedUart::new(
        p.UART0,
        Irqs,
        p.PIN_0,
        p.PIN_1,
        unsafe { &mut *core::ptr::addr_of_mut!(TX_BUF) },
        unsafe { &mut *core::ptr::addr_of_mut!(RX_BUF) },
        uart_config,
    );
    
    let (mut _tx, mut rx) = uart.split();

    info!("UART Serial ready on GPIO 0 (TX), GPIO 1 (RX) at 115200 baud");

    // IR LED connected to GPIO 15 (Pin 20 on RP2350)
    // Connect through a current-limiting resistor (e.g., 100-330 ohm)
    let mut ir_led = Output::new(p.PIN_15, Level::High); // Start with LED off

    info!("IR LED on GPIO 15");
    info!("Loaded {} signals", IR_SIGNALS.len());
    
    // Print available commands
    info!("Available commands:");
    for signal in IR_SIGNALS.iter() {
        if let Some(colon_pos) = signal.find(':') {
            let name = &signal[..colon_pos];
            info!("  {}", name);
        }
    }

    let mut line_buffer = String::<MAX_LINE_LEN>::new();
    
    loop {
        // Read from UART
        let mut buf = [0u8; 1];
        
        match rx.read(&mut buf).await {
            Ok(_) => {
                let ch = buf[0] as char;
                
                if ch == '\n' || ch == '\r' {
                    if !line_buffer.is_empty() {
                        info!("Received command: {}", line_buffer.as_str());
                        
                        // Try to find matching signal
                        let mut found = false;
                        
                        for signal in IR_SIGNALS.iter() {
                            if let Some(colon_pos) = signal.find(':') {
                                let name = &signal[..colon_pos];
                                
                                if name.eq_ignore_ascii_case(line_buffer.as_str().trim()) {
                                    info!("Matched signal: {}", name);
                                    
                                    // Parse and transmit
                                    if let Some(ir_signal) = IrSignal::from_text(signal) {
                                        transmit_ir_signal(&mut ir_led, &ir_signal.pulses).await;
                                        found = true;
                                    } else {
                                        error!("Failed to parse signal: {}", name);
                                    }
                                    break;
                                }
                            }
                        }
                        
                        // Also support direct IR_SIGNAL format from decoder
                        if !found && line_buffer.starts_with("IR_SIGNAL:") {
                            info!("Parsing direct IR signal format");
                            if let Some(ir_signal) = IrSignal::from_text(line_buffer.as_str()) {
                                transmit_ir_signal(&mut ir_led, &ir_signal.pulses).await;
                                found = true;
                            }
                        }
                        
                        if !found {
                            warn!("Unknown command: {}", line_buffer.as_str());
                        }
                        
                        line_buffer.clear();
                    }
                } else if ch.is_ascii() && !ch.is_ascii_control() {
                    if line_buffer.push(ch).is_err() {
                        warn!("Line buffer full, clearing");
                        line_buffer.clear();
                    }
                }
            }
            Err(e) => {
                error!("UART read error: {:?}", e);
                Timer::after(Duration::from_millis(100)).await;
            }
        }
    }
}
