use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperCnrom {
    cart: Cartridge,
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: usize,
    chr_bank: usize,
    prg_len: usize,
    chr_len: usize,
    last_prg_offset: usize,
    chr_bank_offset: usize,
    chr_power_of_two: bool,
    chr_mask: usize,
    num_prg_banks: usize,
    num_chr_banks: usize,
}

impl MapperCnrom {
    pub const ID: u8 = 3;

    pub fn new(cart: Cartridge) -> MapperCnrom {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        let prg_len = cart.prg_rom.len();
        let chr_len = cart.chr_rom.len();
        let last_prg_offset = prg_len.saturating_sub(0x4000);
        let num_prg_banks = prg_len / 0x4000;
        let num_chr_banks = chr_len / 0x2000;
        let chr_power_of_two = chr_len.is_power_of_two();
        let chr_mask = if chr_power_of_two { chr_len - 1 } else { 0 };
        MapperCnrom {
            cart,
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
            chr_bank: 0,
            prg_len,
            chr_len,
            last_prg_offset,
            chr_bank_offset: 0,
            chr_power_of_two,
            chr_mask,
            num_prg_banks,
            num_chr_banks,
        }
    }
}

impl Mapper for MapperCnrom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                if self.chr_len == 0 { return 0; }
                let idx = if self.chr_power_of_two {
                    (self.chr_bank_offset + (addr as usize)) & self.chr_mask
                } else {
                    (self.chr_bank_offset + (addr as usize)) % self.chr_len
                };
                self.cart.chr_rom[idx]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x8000..=0xBFFF => {
                let bank = self.prg_bank * 0x4000;
                self.cart.prg_rom[(bank + (addr & 0x3FFF) as usize) % self.prg_len]
            }
            0xC000..=0xFFFF => {
                self.cart.prg_rom[self.last_prg_offset + (addr & 0x3FFF) as usize]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if self.chr_len > 0 {
                    let idx = if self.chr_power_of_two {
                        (self.chr_bank_offset + (addr as usize)) & self.chr_mask
                    } else {
                        (self.chr_bank_offset + (addr as usize)) % self.chr_len
                    };
                    self.cart.chr_rom[idx] = val;
                }
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            0x8000..=0xFFFF => {
                self.prg_bank = (val as usize) % self.num_prg_banks;
                if self.num_chr_banks > 0 {
                    self.chr_bank = (val as usize) % self.num_chr_banks;
                    self.chr_bank_offset = self.chr_bank * 0x2000;
                }
            }
            _ => {}
        };
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.prg_len = cartridge.prg_rom.len();
        self.chr_len = cartridge.chr_rom.len();
        self.last_prg_offset = self.prg_len.saturating_sub(0x4000);
        self.num_prg_banks = self.prg_len / 0x4000;
        self.num_chr_banks = self.chr_len / 0x2000;
        self.chr_power_of_two = self.chr_len.is_power_of_two();
        self.chr_mask = if self.chr_power_of_two { self.chr_len - 1 } else { 0 };
        self.chr_bank_offset = self.chr_bank * 0x2000;
        self.cart = cartridge;
    }

    fn get_prg_rom_offset(&self, addr: u16) -> Option<u32> {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank * 0x4000;
                Some(((bank + (addr & 0x3FFF) as usize) % self.prg_len) as u32)
            }
            0xC000..=0xFFFF => Some((self.last_prg_offset + (addr & 0x3FFF) as usize) as u32),
            _ => None,
        }
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.vram.write(writer)?;
        self.mirror_mode.write(writer)?;
        self.prg_bank.write(writer)?;
        self.chr_bank.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        self.prg_bank = ReadState::read(reader)?;
        self.chr_bank = ReadState::read(reader)?;
        Ok(())
    }
}
