#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::uart::{Config as UartConfig, UartTx};
use embassy_time::{Duration, Timer};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    
    info!("Hello World - Starting");
    
    // Add a delay to ensure initialization completes
    Timer::after(Duration::from_millis(100)).await;

    // Configure UART0 for serial communication
    // TX on GPIO 0 (Pin 1)
    use embassy_rp::uart::{DataBits, Parity, StopBits};
    
    let mut uart_config = UartConfig::default();
    uart_config.baudrate = 115200;
    uart_config.data_bits = DataBits::DataBits8;
    uart_config.stop_bits = StopBits::STOP1;
    uart_config.parity = Parity::ParityNone;
    
    let mut uart = UartTx::new_blocking(p.UART0, p.PIN_0, uart_config);

    info!("UART Serial ready on GPIO 0 (TX) at 115200 baud");

    let mut counter: u32 = 1;
    
    loop {
        // Format the message
        let mut buffer = heapless::String::<64>::new();
        let _ = core::fmt::write(&mut buffer, format_args!("Hello World {}\r\n", counter));
        
        // Send over UART
        uart.blocking_write(buffer.as_bytes()).unwrap();
        
        info!("Sent: Hello World {}", counter);
        
        counter += 1;
        
        // Wait 1 second
        Timer::after(Duration::from_secs(1)).await;
    }
}
