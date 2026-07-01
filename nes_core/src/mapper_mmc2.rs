use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use serde::{Deserialize, Serialize};
use serde_big_array::big_array;

big_array! { BigArray; 2048, 8192 }

#[derive(Serialize, Deserialize)]
pub struct MapperMmc2 {
    #[serde(skip)]
    cart: Cartridge,
    #[serde(with = "BigArray")]
    ram: [u8; 8192],
    #[serde(with = "BigArray")]
    vram: [u8; 2048],
    mirror_mode: MirrorMode,

    prg_bank_8000: usize,
    chr_bank_0_fd: usize,
    chr_bank_0_fe: usize,
    chr_bank_1_fd: usize,
    chr_bank_1_fe: usize,

    // true = FE mode, false = FD mode
    latch_0: bool, 
    latch_1: bool,
}

impl MapperMmc2 {
    pub const ID: u8 = 9;

    pub fn new(cart: Cartridge) -> MapperMmc2 {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorVertical,
            1 => MirrorMode::MirrorHorizontal,
            _ => MirrorMode::MirrorVertical, // Safe fallback
        };
        
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
        }
    }
}

impl Mapper for MapperMmc2 {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU CHR Bank 0 (Sprites)
            0x0000..=0x0FFF => {
                let bank = if self.latch_0 {
                    self.chr_bank_0_fe
                } else {
                    self.chr_bank_0_fd
                };
                
                let chr_banks = self.cart.chr_rom.len() / 0x1000;
                let offset = (bank % chr_banks) * 0x1000;
                let data = self.cart.chr_rom[offset + (addr & 0x0FFF) as usize];
                
                // PPU Snooping: Swap banks AFTER the read completes
                if addr == 0x0FD8 {
                    self.latch_0 = false;
                } else if addr == 0x0FE8 {
                    self.latch_0 = true;
                }
                
                data
            }
            
            // PPU CHR Bank 1 (Background)
            0x1000..=0x1FFF => {
                let bank = if self.latch_1 {
                    self.chr_bank_1_fe
                } else {
                    self.chr_bank_1_fd
                };
                
                let chr_banks = self.cart.chr_rom.len() / 0x1000;
                let offset = (bank % chr_banks) * 0x1000;
                let data = self.cart.chr_rom[offset + (addr & 0x0FFF) as usize];
                
                // PPU Snooping: Latch 1 checks a small range of addresses
                if (addr & 0xFFF8) == 0x1FD8 {
                    self.latch_1 = false;
                } else if (addr & 0xFFF8) == 0x1FE8 {
                    self.latch_1 = true;
                }
                
                data
            }
            
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            // CPU RAM
            0x6000..=0x7FFF => self.ram[(addr & 0x1FFF) as usize],
            
            // CPU PRG Banks (8 KB each)
            0x8000..=0xFFFF => {
                let prg_banks = self.cart.prg_rom.len() / 0x2000;
                let bank = match addr {
                    0x8000..=0x9FFF => self.prg_bank_8000 % prg_banks,
                    0xA000..=0xBFFF => prg_banks.saturating_sub(3), // Fixed to third-to-last
                    0xC000..=0xDFFF => prg_banks.saturating_sub(2), // Fixed to second-to-last
                    _ /* 0xE000..=0xFFFF */ => prg_banks.saturating_sub(1), // Fixed to last
                };
                
                let offset = bank * 0x2000;
                self.cart.prg_rom[offset + (addr & 0x1FFF) as usize]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            // PPU CHR 
            0x0000..=0x0FFF => {
                let bank = if self.latch_0 { self.chr_bank_0_fe } else { self.chr_bank_0_fd };
                let chr_banks = self.cart.chr_rom.len() / 0x1000;
                let offset = (bank % chr_banks) * 0x1000;
                self.cart.chr_rom[offset + (addr & 0x0FFF) as usize] = val;
            }
            0x1000..=0x1FFF => {
                let bank = if self.latch_1 { self.chr_bank_1_fe } else { self.chr_bank_1_fd };
                let chr_banks = self.cart.chr_rom.len() / 0x1000;
                let offset = (bank % chr_banks) * 0x1000;
                self.cart.chr_rom[offset + (addr & 0x0FFF) as usize] = val;
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            // CPU RAM
            0x6000..=0x7FFF => self.ram[(addr & 0x1FFF) as usize] = val,

            // CPU Registers
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
        self.cart = cartridge;
    }
}