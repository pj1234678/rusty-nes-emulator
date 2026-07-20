use std::io::{Read, Write};

use crate::save_state::{ReadState, WriteState};

const DUTY_TABLE: [[bool; 8]; 4] = [
    [false, false, false, false, false, false, false, true],
    [false, false, false, false, false, false, true, true],
    [false, false, false, false, true, true, true, true],
    [true, true, true, true, true, true, false, false],
];

pub struct Pulse {
    enabled: bool,
    duty_table: usize,
    duty_counter: usize,
    volume: u8,
    freq_counter: u16,
    freq_timer: u16,
    length_counter: u8,
    length_enabled: bool,
    decay_enabled: bool,
    decay_reset_flag: bool,
    decay_hidden_volume: u8,
    decay_counter: u8,
    decay_loop: bool,
    sweep_timer: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_reload: bool,
    sweep_enabled: bool,
    sweep_counter: u8,

    // 1 for Pulse 1, 0 for Pulse 2.
    sweep_negate_constant: u16,
}

impl WriteState for Pulse {
    fn write(&self, writer: &mut dyn Write) -> std::io::Result<()> {
        self.enabled.write(writer)?;
        self.duty_table.write(writer)?;
        self.duty_counter.write(writer)?;
        self.volume.write(writer)?;
        self.freq_counter.write(writer)?;
        self.freq_timer.write(writer)?;
        self.length_counter.write(writer)?;
        self.length_enabled.write(writer)?;
        self.decay_enabled.write(writer)?;
        self.decay_reset_flag.write(writer)?;
        self.decay_hidden_volume.write(writer)?;
        self.decay_counter.write(writer)?;
        self.decay_loop.write(writer)?;
        self.sweep_timer.write(writer)?;
        self.sweep_negate.write(writer)?;
        self.sweep_shift.write(writer)?;
        self.sweep_reload.write(writer)?;
        self.sweep_enabled.write(writer)?;
        self.sweep_counter.write(writer)?;
        self.sweep_negate_constant.write(writer)
    }
}

impl ReadState for Pulse {
    fn read(reader: &mut dyn Read) -> std::io::Result<Self> {
        Ok(Pulse {
            enabled: ReadState::read(reader)?,
            duty_table: ReadState::read(reader)?,
            duty_counter: ReadState::read(reader)?,
            volume: ReadState::read(reader)?,
            freq_counter: ReadState::read(reader)?,
            freq_timer: ReadState::read(reader)?,
            length_counter: ReadState::read(reader)?,
            length_enabled: ReadState::read(reader)?,
            decay_enabled: ReadState::read(reader)?,
            decay_reset_flag: ReadState::read(reader)?,
            decay_hidden_volume: ReadState::read(reader)?,
            decay_counter: ReadState::read(reader)?,
            decay_loop: ReadState::read(reader)?,
            sweep_timer: ReadState::read(reader)?,
            sweep_negate: ReadState::read(reader)?,
            sweep_shift: ReadState::read(reader)?,
            sweep_reload: ReadState::read(reader)?,
            sweep_enabled: ReadState::read(reader)?,
            sweep_counter: ReadState::read(reader)?,
            sweep_negate_constant: ReadState::read(reader)?,
        })
    }
}

impl Pulse {
    pub fn new_pulse1() -> Pulse {
        Pulse::new(1)
    }

    pub fn new_pulse2() -> Pulse {
        Pulse::new(0)
    }

    fn new(sweep_negate_constant: u16) -> Pulse {
        Pulse {
            enabled: false,
            duty_table: 0,
            duty_counter: 0,
            volume: 0,
            freq_counter: 0,
            freq_timer: 0,
            length_counter: 0,
            length_enabled: false,
            decay_enabled: false,
            decay_reset_flag: false,
            decay_hidden_volume: 0,
            decay_counter: 0,
            decay_loop: false,
            sweep_timer: 0,
            sweep_negate: false,
            sweep_shift: 0,
            sweep_reload: false,
            sweep_enabled: false,
            sweep_counter: 0,
            sweep_negate_constant,
        }
    }

    /// Clocked every APU cycle (every 2 CPU cycles).
    #[inline]
    pub fn clock(&mut self) {
        if self.freq_counter > 0 {
            self.freq_counter -= 1;
        } else {
            self.freq_counter = self.freq_timer;
            self.duty_counter = (self.duty_counter + 1) & 0x7;
        }
    }

    #[inline]
    pub fn clock_frame_quarter(&mut self) {
        // Envelope
        if self.decay_reset_flag {
            self.decay_reset_flag = false;
            self.decay_hidden_volume = 0xF;
            self.decay_counter = self.volume;
        } else {
            if self.decay_counter > 0 {
                self.decay_counter -= 1;
            } else {
                self.decay_counter = self.volume;
                if self.decay_hidden_volume > 0 {
                    self.decay_hidden_volume -= 1;
                } else if self.decay_loop {
                    self.decay_hidden_volume = 0xF;
                }
            }
        }
    }

    #[inline]
    pub fn clock_frame_half(&mut self) {
        // Clock Sweep.
        if self.sweep_reload {
            self.sweep_counter = self.sweep_timer;
            self.sweep_reload = false;
        } else if self.sweep_counter > 0 {
            self.sweep_counter -= 1;
        } else {
            self.sweep_counter = self.sweep_timer;

            if self.sweep_enabled && !self.is_sweep_silencing() {
                if self.sweep_negate {
                    self.freq_timer -= self.freq_timer >> self.sweep_shift;
                    self.freq_timer -= self.sweep_negate_constant;
                } else {
                    self.freq_timer += self.freq_timer >> self.sweep_shift;
                }
            }
        }

        // Clock Length.
        if self.length_enabled && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    #[inline]
    pub fn output(&self) -> u8 {
        if DUTY_TABLE[self.duty_table][self.duty_counter]
            && self.length_counter != 0
            && !self.is_sweep_silencing()
        {
            if self.decay_enabled {
                self.decay_hidden_volume
            } else {
                self.volume
            }
        } else {
            0
        }
    }

    #[inline]
    fn is_sweep_silencing(&self) -> bool {
        if self.freq_timer < 8 {
            true
        } else if !self.sweep_negate
            && (self.freq_timer + (self.freq_timer >> self.sweep_shift)) >= 0x800
        {
            true
        } else {
            false
        }
    }

    pub fn poke_register(&mut self, register: u16, data: u8) {
        match register & 0b11 {
            0 => {
                self.duty_table = ((data & 0b1100_0000) >> 6) as usize;
                self.volume = data & 0b0000_1111;
                self.length_enabled = (data & 0b0010_0000) == 0;
                self.decay_enabled = (data & 0b0001_0000) == 0;
                self.decay_loop = (data & 0b0010_0000) != 0;
            }
            1 => {
                self.sweep_timer = (data & 0b0111_0000) >> 4;
                self.sweep_negate = (data & 0b0000_1000) != 0;
                self.sweep_shift = data & 0b0000_0111;
                self.sweep_reload = true;
                self.sweep_enabled = ((data & 0b1000_0000) != 0) && (self.sweep_shift != 0);
            }
            2 => {
                self.freq_timer &= 0xFF00;
                self.freq_timer |= data as u16;
            }
            3 => {
                self.freq_timer &= 0x00FF;
                self.freq_timer |= ((data as u16) & 0b111) << 8;

                if self.enabled {
                    self.length_counter = super::LENGTH_TABLE[(data >> 3) as usize];
                }

                self.freq_counter = self.freq_timer;
                self.duty_counter = 0;
                self.decay_reset_flag = true;
            }
            _ => unreachable!(),
        }
    }

    pub fn set_enable_flag(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !self.enabled {
            self.length_counter = 0;
        }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.length_counter > 0
    }
}