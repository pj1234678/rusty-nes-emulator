use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperMmc4 {
    cart: Cartridge,
    ram: [u8; 8192],
    vram: [u8; 2048],
    mirror_mode: MirrorMode,

    prg_bank: usize,
    chr_bank_0_fd: usize,
    chr_bank_0_fe: usize,
    chr_bank_1_fd: usize,
    chr_bank_1_fe: usize,

    latch_0: bool,
    latch_1: bool,

    irq_enabled: bool,
    irq_pending: bool,
    irq_latch: u8,
    irq_counter: u8,
}

impl MapperMmc4 {
    pub const ID: u8 = 65;

    pub fn new(cart: Cartridge) -> MapperMmc4 {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorVertical,
            1 => MirrorMode::MirrorHorizontal,
            _ => MirrorMode::MirrorVertical,
        };

        MapperMmc4 {
            cart,
            ram: [0; 8192],
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
            chr_bank_0_fd: 0,
            chr_bank_0_fe: 0,
            chr_bank_1_fd: 0,
            chr_bank_1_fe: 0,
            latch_0: false,
            latch_1: false,
            irq_enabled: false,
            irq_pending: false,
            irq_latch: 0,
            irq_counter: 0,
        }
    }
}

impl Mapper for MapperMmc4 {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x0FFF => {
                let bank = if self.latch_0 {
                    self.chr_bank_0_fe
                } else {
                    self.chr_bank_0_fd
                };

                let chr_banks = self.cart.chr_rom.len() / 0x1000;
                if chr_banks == 0 { return 0; }
                let offset = (bank % chr_banks) * 0x1000;
                let data = self.cart.chr_rom[offset + (addr & 0x0FFF) as usize];

                if (addr & 0xFFF8) == 0x0FD8 {
                    self.latch_0 = false;
                } else if (addr & 0xFFF8) == 0x0FE8 {
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

                let chr_banks = self.cart.chr_rom.len() / 0x1000;
                if chr_banks == 0 { return 0; }
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
                let prg_banks = self.cart.prg_rom.len() / 0x2000;
                if prg_banks == 0 { return 0; }
                let bank = match addr {
                    0x8000..=0x9FFF => self.prg_bank % prg_banks,
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
            0x8000..=0x8FFF => {
                self.irq_latch = val;
            }
            0x9000..=0x9FFF => {
                self.irq_counter = self.irq_latch;
                self.irq_enabled = true;
                self.irq_pending = false;
            }
            0xA000..=0xAFFF => self.prg_bank = (val & 0x0F) as usize,
            0xB000..=0xBFFF => self.chr_bank_0_fd = (val & 0x1F) as usize,
            0xC000..=0xCFFF => self.chr_bank_0_fe = (val & 0x1F) as usize,
            0xD000..=0xDFFF => self.chr_bank_1_fd = (val & 0x1F) as usize,
            0xE000..=0xEFFF => {
                self.chr_bank_1_fe = (val & 0x1F) as usize;
                self.mirror_mode = if (val & 0x01) == 0 {
                    MirrorMode::MirrorHorizontal
                } else {
                    MirrorMode::MirrorVertical
                };
            }
            0xF000..=0xFFFF => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            _ => {}
        };
    }

    fn check_irq(&self) -> bool {
        self.irq_pending
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.cart = cartridge;
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
        self.prg_bank.write(writer)?;
        self.chr_bank_0_fd.write(writer)?;
        self.chr_bank_0_fe.write(writer)?;
        self.chr_bank_1_fd.write(writer)?;
        self.chr_bank_1_fe.write(writer)?;
        self.latch_0.write(writer)?;
        self.latch_1.write(writer)?;
        self.irq_enabled.write(writer)?;
        self.irq_pending.write(writer)?;
        self.irq_latch.write(writer)?;
        self.irq_counter.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.ram = ReadState::read(reader)?;
        self.vram = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        self.prg_bank = ReadState::read(reader)?;
        self.chr_bank_0_fd = ReadState::read(reader)?;
        self.chr_bank_0_fe = ReadState::read(reader)?;
        self.chr_bank_1_fd = ReadState::read(reader)?;
        self.chr_bank_1_fe = ReadState::read(reader)?;
        self.latch_0 = ReadState::read(reader)?;
        self.latch_1 = ReadState::read(reader)?;
        self.irq_enabled = ReadState::read(reader)?;
        self.irq_pending = ReadState::read(reader)?;
        self.irq_latch = ReadState::read(reader)?;
        self.irq_counter = ReadState::read(reader)?;
        Ok(())
    }
}
