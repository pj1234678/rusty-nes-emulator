use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use serde::{Deserialize, Serialize};
use serde_big_array::big_array;

big_array! { BigArray; 2048 }

#[derive(Serialize, Deserialize)]
pub struct MapperAxRom {
    #[serde(skip)]
    cart: Cartridge,
    #[serde(with = "BigArray")]
    vram: [u8; 2048],

    prg_bank: usize,
    mirror_mode: MirrorMode,
}

impl MapperAxRom {
    pub const ID: u8 = 7;

pub fn new(cart: Cartridge) -> MapperAxRom {
        // Default to the last bank so the CPU can find the correct reset vector.
        let last_bank = if cart.prg_rom.is_empty() {
            0
        } else {
            (cart.prg_rom.len() / 0x8000).saturating_sub(1)
        };

        MapperAxRom {
            cart,
            vram: [0; 2048],
            prg_bank: last_bank,
            mirror_mode: MirrorMode::MirrorSingleA, 
        }
    }
}

impl Mapper for MapperAxRom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU 
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize],
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            // CPU
            0x8000..=0xFFFF => {
                // 32 KiB PRG ROM Bank
                let offset = self.prg_bank * 0x8000;
                let masked_addr = (addr & 0x7FFF) as usize;
                
                // Modulo by PRG ROM length to prevent panics on bad headers/oversized reads
                self.cart.prg_rom[(offset + masked_addr) % self.cart.prg_rom.len()]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            // PPU
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize] = val, // AxROM typically uses CHR RAM
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            // CPU
            0x8000..=0xFFFF => {
                // Bits 0-2 select the 32 KiB PRG bank
                self.prg_bank = (val & 0x07) as usize;
                
                // Bit 4 selects the VRAM page for single-screen mirroring
                self.mirror_mode = if (val & 0x10) != 0 {
                    MirrorMode::MirrorSingleB
                } else {
                    MirrorMode::MirrorSingleA
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