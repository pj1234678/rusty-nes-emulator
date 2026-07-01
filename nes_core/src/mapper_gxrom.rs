use super::cartridge::Cartridge;
use super::mapper::{translate_vram, Mapper, MirrorMode};
use serde::{Deserialize, Serialize};
use serde_big_array::big_array;

big_array! { BigArray; 2048 }

#[derive(Serialize, Deserialize)]
pub struct MapperGxRom {
    #[serde(skip)]
    cart: Cartridge,
    #[serde(with = "BigArray")]
    vram: [u8; 2048],
    mirror_mode: MirrorMode,
    prg_bank: usize,
    chr_bank: usize,
}

impl MapperGxRom {
    pub const ID: u8 = 66;

    pub fn new(cart: Cartridge) -> MapperGxRom {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };
        MapperGxRom {
            cart,
            vram: [0; 2048],
            mirror_mode,
            prg_bank: 0,
            chr_bank: 0,
        }
    }
}

impl Mapper for MapperGxRom {
    fn peek(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU
            0x0000..=0x1FFF => {
                let bank = self.chr_bank * 0x2000;
                self.cart.chr_rom[bank + (addr as usize) % self.cart.chr_rom.len()]
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)],

            // CPU - 32KB PRG Bank mapping!
            0x8000..=0xFFFF => {
                let bank = self.prg_bank * 0x8000;
                let offset = addr as usize - 0x8000;
                self.cart.prg_rom[(bank + offset) % self.cart.prg_rom.len()]
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        match addr {
            // PPU
            0x0000..=0x1FFF => {
                let bank = self.chr_bank * 0x2000;
                let len = self.cart.chr_rom.len();
                self.cart.chr_rom[(bank + (addr as usize)) % len] = val;
            }
            0x2000..=0x3EFF => self.vram[translate_vram(self.mirror_mode, addr)] = val,

            // CPU - bank select register
            0x8000..=0xFFFF => {
                // Bits 4-5 are PRG, Bits 0-1 are CHR
                self.prg_bank = ((val >> 4) & 0x03) as usize;
                self.chr_bank = (val & 0x03) as usize;
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
