use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperMmc2 {
    cart: Cartridge,
    ram: [u8; 8192],
    vram: [u8; 2048],
    mirror_mode: MirrorMode,

    prg_bank_8000: usize,
    chr_bank_0_fd: usize,
    chr_bank_0_fe: usize,
    chr_bank_1_fd: usize,
    chr_bank_1_fe: usize,

    latch_0: bool,
    latch_1: bool,

    prg_banks: usize,
    chr_banks: usize,
}

impl MapperMmc2 {
    pub const ID: u8 = 9;

    pub fn new(cart: Cartridge) -> MapperMmc2 {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorVertical,
            1 => MirrorMode::MirrorHorizontal,
            _ => MirrorMode::MirrorVertical,
        };

        let prg_banks = cart.prg_rom.len() / 0x2000;
        let chr_banks = cart.chr_rom.len() / 0x1000;
        MapperMmc2 {
            cart,
            ram: [0; 8192],
            vram: [0; 2048],
            mirror_mode,
            prg_bank_8000: 0,
            chr_bank_0_fd: 0,
            chr_bank_0_fe: 0,
            chr_bank_1_fd: 0,
            chr_bank_1_fe: 0,
            latch_0: false,
            latch_1: false,
            prg_banks,
            chr_banks,
        }
    }
}

impl Mapper for MapperMmc2 {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x0FFF => {
                let bank = if self.latch_0 {
                    self.chr_bank_0_fe
                } else {
                    self.chr_bank_0_fd
                };

                let chr_banks = self.chr_banks;
                let offset = (bank % chr_banks) * 0x1000;
                let data = self.cart.chr_rom[offset + (addr & 0x0FFF) as usize];

                if addr == 0x0FD8 {
                    self.latch_0 = false;
                } else if addr == 0x0FE8 {
                    self.latch_0 = true;
                }

                data
            }

            0x1000..=0x1FFF => {
                let bank = if self.latch_1 {
                    self.chr_bank_1_fe
                } else {
                    self.chr_bank_1_fd
                };

                let chr_banks = self.chr_banks;
                let offset = (bank % chr_banks) * 0x1000;
                let data = self.cart.chr_rom[offset + (addr & 0x0FFF) as usize];

                if (addr & 0xFFF8) == 0x1FD8 {
                    self.latch_1 = false;
                } else if (addr & 0xFFF8) == 0x1FE8 {
                    self.latch_1 = true;
                }

                data
            }

            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            0x6000..=0x7FFF => self.ram[(addr & 0x1FFF) as usize],

            0x8000..=0xFFFF => {
                let prg_banks = self.prg_banks;
                let bank = match addr {
                    0x8000..=0x9FFF => self.prg_bank_8000 % prg_banks,
                    0xA000..=0xBFFF => prg_banks.saturating_sub(3),
                    0xC000..=0xDFFF => prg_banks.saturating_sub(2),
                    _ => prg_banks.saturating_sub(1),
                };

                let offset = bank * 0x2000;
                self.cart.prg_rom[offset + (addr & 0x1FFF) as usize]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x0FFF => {
                let bank = if self.latch_0 { self.chr_bank_0_fe } else { self.chr_bank_0_fd };
                let chr_banks = self.chr_banks;
                let offset = (bank % chr_banks) * 0x1000;
                self.cart.chr_rom[offset + (addr & 0x0FFF) as usize] = val;
            }
            0x1000..=0x1FFF => {
                let bank = if self.latch_1 { self.chr_bank_1_fe } else { self.chr_bank_1_fd };
                let chr_banks = self.chr_banks;
                let offset = (bank % chr_banks) * 0x1000;
                self.cart.chr_rom[offset + (addr & 0x0FFF) as usize] = val;
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            0x6000..=0x7FFF => self.ram[(addr & 0x1FFF) as usize] = val,

            0xA000..=0xAFFF => self.prg_bank_8000 = (val & 0x0F) as usize,
            0xB000..=0xBFFF => self.chr_bank_0_fd = (val & 0x1F) as usize,
            0xC000..=0xCFFF => self.chr_bank_0_fe = (val & 0x1F) as usize,
            0xD000..=0xDFFF => self.chr_bank_1_fd = (val & 0x1F) as usize,
            0xE000..=0xEFFF => self.chr_bank_1_fe = (val & 0x1F) as usize,
            0xF000..=0xFFFF => {
                self.mirror_mode = if (val & 0x01) == 0 {
                    MirrorMode::MirrorVertical
                } else {
                    MirrorMode::MirrorHorizontal
                };
            }
            _ => {}
        };
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.prg_banks = cartridge.prg_rom.len() / 0x2000;
        self.chr_banks = cartridge.chr_rom.len() / 0x1000;
        self.cart = cartridge;
    }

    fn get_prg_rom_offset(&self, addr: u16) -> Option<u32> {
        match addr {
            0x8000..=0xFFFF => {
                let bank = match addr {
                    0x8000..=0x9FFF => self.prg_bank_8000 % self.prg_banks.max(1),
                    0xA000..=0xBFFF => self.prg_banks.saturating_sub(3),
                    0xC000..=0xDFFF => self.prg_banks.saturating_sub(2),
                    _ => self.prg_banks.saturating_sub(1),
                };
                Some((bank * 0x2000 + (addr & 0x1FFF) as usize) as u32)
            }
            _ => None,
        }
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
        self.mirror_mode.write(writer)?;
        self.prg_bank_8000.write(writer)?;
        self.chr_bank_0_fd.write(writer)?;
        self.chr_bank_0_fe.write(writer)?;
        self.chr_bank_1_fd.write(writer)?;
        self.chr_bank_1_fe.write(writer)?;
        self.latch_0.write(writer)?;
        self.latch_1.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.ram = ReadState::read(reader)?;
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        self.prg_bank_8000 = ReadState::read(reader)?;
        self.chr_bank_0_fd = ReadState::read(reader)?;
        self.chr_bank_0_fe = ReadState::read(reader)?;
        self.chr_bank_1_fd = ReadState::read(reader)?;
        self.chr_bank_1_fe = ReadState::read(reader)?;
        self.latch_0 = ReadState::read(reader)?;
        self.latch_1 = ReadState::read(reader)?;
        self.prg_banks = self.cart.prg_rom.len() / 0x2000;
        self.chr_banks = self.cart.chr_rom.len() / 0x1000;
        Ok(())
    }
}
