use std::io::{Read, Write};

use crate::save_state::{ReadState, WriteState};

use crate::{cpu::InterruptKind, nes::State};

/// In units of APU clock.
const RATE_TABLE: [u16; 16] = [
    214, 190, 170, 160, 143, 127, 113, 107, 95, 80, 71, 64, 53, 42, 36, 27,
];

pub struct Dmc {
    freq_counter: u16,
    output: u8,
    loop_flag: bool,
    irq_enabled: bool,
    irq_pending: bool,
    rate: u16,
    load_sample_address: u16,
    load_sample_length: usize,
    output_shift: u8,
    output_shift_bits: usize,
    address: u16,
    bytes_remaining: usize,
    silence: bool,
    sample_buffer: u8,
    sample_buffer_empty: bool,
}

impl WriteState for Dmc {
    fn write(&self, writer: &mut dyn Write) -> std::io::Result<()> {
        self.freq_counter.write(writer)?;
        self.output.write(writer)?;
        self.loop_flag.write(writer)?;
        self.irq_enabled.write(writer)?;
        self.irq_pending.write(writer)?;
        self.rate.write(writer)?;
        self.load_sample_address.write(writer)?;
        self.load_sample_length.write(writer)?;
        self.output_shift.write(writer)?;
        self.output_shift_bits.write(writer)?;
        self.address.write(writer)?;
        self.bytes_remaining.write(writer)?;
        self.silence.write(writer)?;
        self.sample_buffer.write(writer)?;
        self.sample_buffer_empty.write(writer)
    }
}

impl ReadState for Dmc {
    fn read(reader: &mut dyn Read) -> std::io::Result<Self> {
        Ok(Dmc {
            freq_counter: ReadState::read(reader)?,
            output: ReadState::read(reader)?,
            loop_flag: ReadState::read(reader)?,
            irq_enabled: ReadState::read(reader)?,
            irq_pending: ReadState::read(reader)?,
            rate: ReadState::read(reader)?,
            load_sample_address: ReadState::read(reader)?,
            load_sample_length: ReadState::read(reader)?,
            output_shift: ReadState::read(reader)?,
            output_shift_bits: ReadState::read(reader)?,
            address: ReadState::read(reader)?,
            bytes_remaining: ReadState::read(reader)?,
            silence: ReadState::read(reader)?,
            sample_buffer: ReadState::read(reader)?,
            sample_buffer_empty: ReadState::read(reader)?,
        })
    }
}

impl Dmc {
    pub fn new() -> Dmc {
        Dmc {
            freq_counter: 0,
            output: 0,
            loop_flag: false,
            irq_enabled: false,
            irq_pending: false,
            rate: 0,
            load_sample_address: 0,
            load_sample_length: 0,
            output_shift: 0,
            output_shift_bits: 1,
            address: 0,
            bytes_remaining: 0,
            silence: true,
            sample_buffer: 0,
            sample_buffer_empty: true,
        }
    }

    pub fn clock(s: &mut State) {
        // Clock sample buffer.
        let dmc = &mut s.apu.dmc;
        if dmc.freq_counter > 0 {
            dmc.freq_counter -= 1;
        } else {
            dmc.freq_counter = dmc.rate;

            if !dmc.silence {
                if dmc.output_shift & 1 == 0 {
                    // Subtract 2.
                    if dmc.output >= 2 {
                        dmc.output -= 2;
                    }
                } else {
                    // Add 2.
                    if dmc.output <= 125 {
                        dmc.output += 2;
                    }
                }
            }

            dmc.output_shift >>= 1;
            dmc.output_shift_bits -= 1;
            if dmc.output_shift_bits == 0 {
                dmc.output_shift_bits = 8;
                if dmc.sample_buffer_empty {
                    dmc.silence = true;
                } else {
                    dmc.silence = false;
                    dmc.output_shift = dmc.sample_buffer;
                }
                dmc.sample_buffer_empty = true;
            }
        }

        // Load data maybe?
        if dmc.bytes_remaining > 0 && dmc.sample_buffer_empty {
            let address = dmc.address;
            let sample = s.cpu_peek(address);
            let dmc = &mut s.apu.dmc;
            dmc.sample_buffer = sample;

            dmc.sample_buffer_empty = false;
            if dmc.address < 0xFFFF {
                dmc.address += 1;
            } else {
                dmc.address = 0x8000;
            }

            dmc.bytes_remaining -= 1;
            if dmc.bytes_remaining == 0 {
                if dmc.loop_flag {
                    dmc.address = dmc.load_sample_address;
                    dmc.bytes_remaining = dmc.load_sample_length;
                } else if dmc.irq_enabled {
                    dmc.irq_pending = true;
                    if s.cpu.pending_interrupt == InterruptKind::None {
                        s.cpu.pending_interrupt = InterruptKind::IRQ;
                    }
                }
            }
        }
    }

    pub fn clock_frame_quarter(&mut self) {}

    pub fn clock_frame_half(&mut self) {}

    pub fn output(&self) -> u8 {
        self.output
    }

    pub fn poke_register(&mut self, register: u16, data: u8) {
        match register {
            0x4010 => {
                self.irq_enabled = (data & 0b1000_0000) != 0;
                if !self.irq_enabled {
                    self.irq_pending = false;
                }

                self.loop_flag = (data & 0b0100_0000) != 0;
                self.rate = RATE_TABLE[(data & 0b1111) as usize];
            }
            0x4011 => {
                self.output = data & 0b0111_1111;
            }
            0x4012 => {
                self.load_sample_address = 0xC000 | ((data as u16) << 6);
            }
            0x4013 => {
                self.load_sample_length = ((data as usize) << 4) | 1;
            }
            _ => unreachable!(),
        }
    }

    pub fn set_enable_flag(&mut self, enabled: bool) {
        if enabled {
            if self.bytes_remaining == 0 {
                self.address = self.load_sample_address;
                self.bytes_remaining = self.load_sample_length;
            }
        } else {
            self.bytes_remaining = 0;
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.bytes_remaining > 0
    }

    pub fn is_irq_pending(&self) -> bool {
        self.irq_pending
    }
    
    // FIX: Allow the APU to acknowledge and clear the IRQ
    pub fn clear_irq(&mut self) {
        self.irq_pending = false;
    }
}
