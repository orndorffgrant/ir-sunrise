#![no_std]

use heapless::{String, Vec};

// Maximum pulse buffer size
pub const MAX_PULSES: usize = 512;

#[derive(Debug, Clone)]
pub struct IrSignal {
    pub pulses: Vec<u32, MAX_PULSES>,
    pub complete: bool,
}

impl IrSignal {
    pub fn new() -> Self {
        Self {
            pulses: Vec::new(),
            complete: false,
        }
    }

    pub fn add_pulse(&mut self, duration_us: u32) -> Result<(), ()> {
        self.pulses.push(duration_us).map_err(|_| ())
    }

    pub fn to_text(&self) -> heapless::String<8192> {
        let mut output = heapless::String::new();
        
        // Format: "IR_SIGNAL:<count>:<pulse1>,<pulse2>,...\n"
        // Where pulses are in microseconds
        let _ = core::fmt::write(&mut output, format_args!("IR_SIGNAL:{}:", self.pulses.len()));
        
        for (i, &pulse) in self.pulses.iter().enumerate() {
            if i > 0 {
                let _ = core::fmt::write(&mut output, format_args!(","));
            }
            let _ = core::fmt::write(&mut output, format_args!("{}", pulse));
        }
        let _ = core::fmt::write(&mut output, format_args!("\n"));
        
        output
    }

    /// Parse IR signal from text format
    /// Format: "IR_SIGNAL:<count>:<pulse1>,<pulse2>,..."
    /// or: "NAME:<count>:<pulse1>,<pulse2>,..." (for named signals)
    pub fn from_text(text: &str) -> Option<Self> {
        let mut pulses = Vec::new();
        
        // Find the second colon (after count)
        let mut colon_count = 0;
        let mut data_start = 0;
        
        for (i, c) in text.chars().enumerate() {
            if c == ':' {
                colon_count += 1;
                if colon_count == 2 {
                    data_start = i + 1;
                    break;
                }
            }
        }
        
        if colon_count != 2 {
            return None;
        }
        
        // Parse comma-separated durations
        let data = &text[data_start..];
        let mut current_num = String::<16>::new();
        
        for c in data.chars() {
            if c == ',' || c == '\n' || c == '\r' {
                if !current_num.is_empty() {
                    if let Ok(duration) = current_num.parse::<u32>() {
                        if pulses.push(duration).is_err() {
                            return None;
                        }
                    }
                    current_num.clear();
                }
            } else if c.is_ascii_digit() {
                let _ = current_num.push(c);
            }
        }
        
        // Don't forget the last number
        if !current_num.is_empty() {
            if let Ok(duration) = current_num.parse::<u32>() {
                let _ = pulses.push(duration);
            }
        }
        
        if pulses.is_empty() {
            None
        } else {
            Some(Self {
                pulses,
                complete: true,
            })
        }
    }
}

impl Default for IrSignal {
    fn default() -> Self {
        Self::new()
    }
}
