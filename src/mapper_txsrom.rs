use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

fn get_bank_offset(total_size: usize, bank_size: usize, bank: i32) -> usize {
    let banks = (total_size / bank_size) as i32;
    let bank = bank.rem_euclid(banks) as usize;
    bank * bank_size
}

pub struct MapperTxSrom {
    cart: Cartridge,
    ram: [u8; 8192],
    vram: [u8; 2048],

    reg_bank_select: u8,
    reg_bank_data: [u8; 8],
    mirror_mode: MirrorMode,

    offset_prg: [usize; 4],
    offset_chr: [usize; 8],

    irq_enabled: bool,
    irq_pending: bool,
    irq_reload: bool,
    irq_counter: u8,
    irq_latch: u8,
    last_a12: bool,
}

impl MapperTxSrom {
    pub const ID: u8 = 118;

    pub fn new(cart: Cartridge) -> MapperTxSrom {
        let mut mapper = MapperTxSrom {
            cart,
            ram: [0; 8192],
            vram: [0; 2048],

            mirror_mode: MirrorMode::MirrorHorizontal,
            reg_bank_select: 0,
            reg_bank_data: [0; 8],

            offset_prg: [0; 4],
            offset_chr: [0; 8],

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
            [self.reg_bank_data[6] as i8, self.reg_bank_data[7] as i8, -2i8, -1i8]
        } else {
            [-2i8, self.reg_bank_data[7] as i8, self.reg_bank_data[6] as i8, -1i8]
        };
        let prg_len = self.cart.prg_rom.len();
        for i in 0..4 {
            self.offset_prg[i] = get_bank_offset(prg_len, 8 * 1024, prg_banks[i] as i32);
        }

        let chr_mode = self.reg_bank_select & 0b10000000;
        let chr_banks = if chr_mode == 0 {
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
                self.reg_bank_data[4],
                self.reg_bank_data[5],
                self.reg_bank_data[0] & 0xFE,
                (self.reg_bank_data[0] & 0xFE) | 1,
                self.reg_bank_data[1] & 0xFE,
                (self.reg_bank_data[1] & 0xFE) | 1,
            ]
        };
        let chr_len = self.cart.chr_rom.len();
        for i in 0..8 {
            self.offset_chr[i] = get_bank_offset(chr_len, 1024, chr_banks[i] as i32);
        }
    }

    fn write_register(&mut self, addr: u16, val: u8) {
        match addr {
            0x8000..=0x9FFF if (addr & 1 == 0) => {
                self.reg_bank_select = val;
                self.update_banks();
            }
            0x8000..=0x9FFF if (addr & 1 == 1) => {
                let bank = self.reg_bank_select & 0b111;
                self.reg_bank_data[bank as usize] = val;
                self.update_banks();
            }
            0xA000..=0xBFFF if (addr & 1 == 0) => {
                self.mirror_mode = if val & 0x1 == 0 {
                    MirrorMode::MirrorVertical
                } else {
                    MirrorMode::MirrorHorizontal
                };
            }
            0xA000..=0xBFFF if (addr & 1 == 1) => {}
            0xC000..=0xDFFF if (addr & 1 == 0) => self.irq_latch = val,
            0xC000..=0xDFFF if (addr & 1 == 1) => self.irq_reload = true,
            0xE000..=0xFFFF if (addr & 1 == 0) => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0xE000..=0xFFFF if (addr & 1 == 1) => self.irq_enabled = true,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn check_a12(&mut self, addr: u16) {
        let a12 = (addr & 0b1000000000000) > 0;
        if a12 && !self.last_a12 {
            if self.irq_counter == 0 || self.irq_reload {
                self.irq_counter = self.irq_latch;
                self.irq_reload = false;
            } else {
                self.irq_counter -= 1;
            }
            if self.irq_counter == 0 && self.irq_enabled {
                self.irq_pending = true;
            }
        }
        self.last_a12 = a12;
    }
}

impl Mapper for MapperTxSrom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                self.check_a12(addr);
                let bank = ((addr & 0xFC00) >> 10) as usize;
                let offset = (addr & 0x3FF) as usize;
                self.cart.chr_rom[self.offset_chr[bank] + offset]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],
            0x6000..=0x7FFF => self.ram[(addr & 0x1FFF) as usize],
            0x8000..=0xFFFF => {
                let bank = ((addr & 0x6000) >> 13) as usize;
                let offset = (addr & 0x1FFF) as usize;
                self.cart.prg_rom[self.offset_prg[bank] + offset]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.check_a12(addr);
                let bank = ((addr & 0xFC00) >> 10) as usize;
                let offset = (addr & 0x3FF) as usize;
                self.cart.chr_rom[self.offset_chr[bank] + offset] = val;
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,
            0x6000..=0x7FFF => self.ram[(addr & 0x1FFF) as usize] = val,
            0x8000..=0xFFFF => self.write_register(addr, val),
            _ => {}
        };
    }

    #[inline]
    fn check_irq(&self) -> bool {
        self.irq_pending
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.cart = cartridge;
    }

    fn get_prg_rom_offset(&self, addr: u16) -> Option<u32> {
        let slot = ((addr & 0x6000) >> 13) as usize;
        Some((self.offset_prg[slot] + (addr & 0x1FFF) as usize) as u32)
    }

    fn get_sram(&self) -> Option<&[u8]> {
        Some(&self.ram)
    }

    fn set_sram(&mut self, data: &[u8]) {
        let len = usize::min(data.len(), self.ram.len());
        self.ram[..len].copy_from_slice(&data[..len]);
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.ram.write(writer)?;
        self.vram.write(writer)?;
        self.reg_bank_select.write(writer)?;
        self.reg_bank_data.write(writer)?;
        self.mirror_mode.write(writer)?;
        for item in &self.offset_prg {
            item.write(writer)?;
        }
        for item in &self.offset_chr {
            item.write(writer)?;
        }
        self.irq_enabled.write(writer)?;
        self.irq_pending.write(writer)?;
        self.irq_reload.write(writer)?;
        self.irq_counter.write(writer)?;
        self.irq_latch.write(writer)?;
        self.last_a12.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.ram = ReadState::read(reader)?;
        self.vram = ReadState::read(reader)?;
        self.reg_bank_select = ReadState::read(reader)?;
        self.reg_bank_data = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        for item in &mut self.offset_prg {
            *item = ReadState::read(reader)?;
        }
        for item in &mut self.offset_chr {
            *item = ReadState::read(reader)?;
        }
        self.irq_enabled = ReadState::read(reader)?;
        self.irq_pending = ReadState::read(reader)?;
        self.irq_reload = ReadState::read(reader)?;
        self.irq_counter = ReadState::read(reader)?;
        self.irq_latch = ReadState::read(reader)?;
        self.last_a12 = ReadState::read(reader)?;
        Ok(())
    }
}
