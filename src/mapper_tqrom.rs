use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperTqrom {
    cart: Cartridge,
    chr_ram: [u8; 8192],
    vram: [u8; 2048],
    mirror_mode: MirrorMode,

    reg_bank_select: u8,
    reg_bank_data: [u8; 8],

    offset_prg: [usize; 4],
    offset_chr: [usize; 8],
    chr_ram_mode: [bool; 8],

    irq_enabled: bool,
    irq_pending: bool,
    irq_reload: bool,
    irq_counter: u8,
    irq_latch: u8,
    last_a12: bool,
}

fn get_bank_offset(total_size: usize, bank_size: usize, bank: i32) -> usize {
    let banks = (total_size / bank_size) as i32;
    let bank = bank.rem_euclid(banks) as usize;
    bank * bank_size
}

impl MapperTqrom {
    pub const ID: u8 = 119;

    pub fn new(cart: Cartridge) -> MapperTqrom {
        let mut mapper = MapperTqrom {
            cart,
            chr_ram: [0; 8192],
            vram: [0; 2048],
            mirror_mode: MirrorMode::MirrorHorizontal,
            reg_bank_select: 0,
            reg_bank_data: [0; 8],
            offset_prg: [0; 4],
            offset_chr: [0; 8],
            chr_ram_mode: [false; 8],
            irq_enabled: false,
            irq_pending: false,
            irq_reload: false,
            irq_counter: 0,
            irq_latch: 0,
            last_a12: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let prg_mode = self.reg_bank_select & 0b01000000;
        let prg_banks = if prg_mode == 0 {
            [
                self.reg_bank_data[6] as i8,
                self.reg_bank_data[7] as i8,
                -2i8,
                -1i8,
            ]
        } else {
            [
                -2i8,
                self.reg_bank_data[7] as i8,
                self.reg_bank_data[6] as i8,
                -1i8,
            ]
        };
        let prg_len = self.cart.prg_rom.len();
        for i in 0..4 {
            self.offset_prg[i] = get_bank_offset(prg_len, 8 * 1024, prg_banks[i] as i32);
        }

        let chr_mode = self.reg_bank_select & 0b10000000;
        let chr_banks_raw: [u8; 8] = if chr_mode == 0 {
            [
                self.reg_bank_data[0] & 0xFE,
                (self.reg_bank_data[0] & 0xFE) | 1,
                self.reg_bank_data[1] & 0xFE,
                (self.reg_bank_data[1] & 0xFE) | 1,
                self.reg_bank_data[2],
                self.reg_bank_data[3],
                self.reg_bank_data[4],
                self.reg_bank_data[5],
            ]
        } else {
            [
                self.reg_bank_data[2],
                self.reg_bank_data[3],
                self.reg_bank_data[4] & 0xFE,
                (self.reg_bank_data[4] & 0xFE) | 1,
                self.reg_bank_data[5] & 0xFE,
                (self.reg_bank_data[5] & 0xFE) | 1,
                self.reg_bank_data[6],
                self.reg_bank_data[7],
            ]
        };

        let chr_len = self.cart.chr_rom.len();
        for i in 0..8 {
            self.chr_ram_mode[i] = chr_banks_raw[i] & 0x40 != 0;
            let bank_val = chr_banks_raw[i] & 0x3F;
            self.offset_chr[i] = if self.chr_ram_mode[i] {
                (bank_val as usize % 4) * 2048
            } else if chr_len > 0 {
                get_bank_offset(chr_len, 1024, bank_val as i32)
            } else {
                0
            };
        }
    }
}

impl Mapper for MapperTqrom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let bank = addr as usize / 1024;
                let offset = (addr & 0x3FF) as usize;
                if self.chr_ram_mode[bank] {
                    self.chr_ram[self.offset_chr[bank] + offset]
                } else if self.cart.chr_rom.is_empty() {
                    0
                } else {
                    self.cart.chr_rom[(self.offset_chr[bank] + offset) % self.cart.chr_rom.len()]
                }
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x8000..=0x9FFF => {
                self.cart.prg_rom[self.offset_prg[0] + (addr & 0x1FFF) as usize]
            }
            0xA000..=0xBFFF => {
                self.cart.prg_rom[self.offset_prg[1] + (addr & 0x1FFF) as usize]
            }
            0xC000..=0xDFFF => {
                self.cart.prg_rom[self.offset_prg[2] + (addr & 0x1FFF) as usize]
            }
            0xE000..=0xFFFF => {
                self.cart.prg_rom[self.offset_prg[3] + (addr & 0x1FFF) as usize]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                let bank = addr as usize / 1024;
                let offset = (addr & 0x3FF) as usize;
                if self.chr_ram_mode[bank] {
                    self.chr_ram[self.offset_chr[bank] + offset] = val;
                }
            }
            0x2000..=0x3EFF => {
                match addr & 0x7 {
                    0 | 2 => self.mirror_mode = if val & 1 == 0 { MirrorMode::MirrorHorizontal } else { MirrorMode::MirrorVertical },
                    _ => {}
                }
            }
            0x4000..=0x5FFF => {}

            0x6000..=0x7FFF => {}

            0x8000..=0x9FFF => {
                match addr & 1 {
                    0 => self.reg_bank_select = val,
                    1 => {
                        self.reg_bank_data[(self.reg_bank_select & 7) as usize] = val;
                        self.update_banks();
                    }
                    _ => {}
                }
            }
            0xA000..=0xBFFF => {
                match addr & 1 {
                    0 => {
                        if val & 1 == 0 {
                            self.mirror_mode = MirrorMode::MirrorVertical;
                        } else {
                            self.mirror_mode = MirrorMode::MirrorHorizontal;
                        }
                    }
                    1 => {}
                    _ => {}
                }
            }
            0xC000..=0xDFFF => {
                match addr & 1 {
                    0 => self.irq_latch = val,
                    1 => {
                        self.irq_counter = self.irq_latch;
                        self.irq_reload = true;
                    }
                    _ => {}
                }
            }
            0xE000..=0xFFFF => {
                match addr & 1 {
                    0 => self.irq_enabled = false,
                    1 => self.irq_enabled = true,
                    _ => {}
                }
            }
            _ => {}
        };
    }

    fn check_irq(&self) -> bool {
        self.irq_enabled && self.irq_pending
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.cart = cartridge;
        self.update_banks();
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.chr_ram.write(writer)?;
        self.vram.write(writer)?;
        self.mirror_mode.write(writer)?;
        self.reg_bank_select.write(writer)?;
        for v in &self.reg_bank_data { v.write(writer)?; }
        for v in &self.offset_prg { v.write(writer)?; }
        for v in &self.offset_chr { v.write(writer)?; }
        for v in &self.chr_ram_mode { v.write(writer)?; }
        self.irq_enabled.write(writer)?;
        self.irq_pending.write(writer)?;
        self.irq_reload.write(writer)?;
        self.irq_counter.write(writer)?;
        self.irq_latch.write(writer)?;
        self.last_a12.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.chr_ram = ReadState::read(reader)?;
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        self.reg_bank_select = ReadState::read(reader)?;
        for v in &mut self.reg_bank_data { *v = ReadState::read(reader)?; }
        for v in &mut self.offset_prg { *v = ReadState::read(reader)?; }
        for v in &mut self.offset_chr { *v = ReadState::read(reader)?; }
        for v in &mut self.chr_ram_mode { *v = ReadState::read(reader)?; }
        self.irq_enabled = ReadState::read(reader)?;
        self.irq_pending = ReadState::read(reader)?;
        self.irq_reload = ReadState::read(reader)?;
        self.irq_counter = ReadState::read(reader)?;
        self.irq_latch = ReadState::read(reader)?;
        self.last_a12 = ReadState::read(reader)?;
        Ok(())
    }
}
