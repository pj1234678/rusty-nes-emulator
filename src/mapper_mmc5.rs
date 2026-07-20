use std::io::{self, Read, Write};

use super::cartridge::Cartridge;
use super::mapper::{Mapper, MirrorMode};
use super::save_state::{ReadState, WriteState};

pub struct MapperMmc5 {
    cart: Cartridge,

    prg_mode: u8,
    prg_bank: [u8; 4],     

    chr_mode: u8,
    chr_upper_bank: u8,    
    chr_bank: [u8; 8],     
    chr_bank_nt: [u8; 4],  

    nametable_mode: u8,    
    vram: [u8; 2048],
    fill_tile: u8,         
    fill_attr: u8,         

    exram_mode: u8,        
    exram: [u8; 1024],

    irq_enabled: bool,
    irq_pending: bool,
    irq_latch: u8,         
    irq_counter: u8,       

    in_frame: bool,
    mul_a: u8,             
    mul_b: u8,             

    wram_bank: u8,
    wram: Vec<u8>,
    wram_enabled: bool,
    prg_ram_write_1: u8,   
    prg_ram_write_2: u8,   

    mirror_mode: MirrorMode,

    nt_latch: u8,
    exram_latch: u8,
    bg_chr_fetches_left: u8,

    cpu_reads_since_ppu: u16,
    last_ppu_address: u16,
    ppu_match_count: u8,

    prg_len: usize,
    chr_len: usize,
}

impl MapperMmc5 {
    pub const ID: u8 = 5;

    pub fn new(cart: Cartridge) -> MapperMmc5 {
        let mirror_mode = match cart.mirror_mode {
            0 => MirrorMode::MirrorHorizontal,
            1 => MirrorMode::MirrorVertical,
            _ => MirrorMode::MirrorHorizontal,
        };

        let wram_size = 64 * 1024;

        let prg_len = cart.prg_rom.len();
        let chr_len = cart.chr_rom.len();

        MapperMmc5 {
            cart,
            prg_mode: 0x03,
            prg_bank: [0xFF; 4],

            chr_mode: 0x03,
            chr_upper_bank: 0,
            chr_bank: [0; 8],
            chr_bank_nt: [0; 4],

            nametable_mode: 0x00,
            vram: [0; 2048],
            fill_tile: 0,
            fill_attr: 0,

            exram_mode: 0x00,
            exram: [0; 1024],

            irq_enabled: false,
            irq_pending: false,
            irq_latch: 0,
            irq_counter: 0,

            in_frame: false,
            mul_a: 0,
            mul_b: 0,

            wram_bank: 0,
            wram: vec![0; wram_size],
            wram_enabled: true,
            prg_ram_write_1: 0,
            prg_ram_write_2: 0,

            mirror_mode,

            nt_latch: 0,
            exram_latch: 0,
            bg_chr_fetches_left: 0,
            
            cpu_reads_since_ppu: 0,
            last_ppu_address: 0xFFFF,
            ppu_match_count: 0,
            prg_len,
            chr_len,
        }
    }

    fn prg_bank_offset(&self, bank: u8, bank_size: usize) -> usize {
        let bank_num = (bank & 0x7F) as usize;
        let rom_size = self.prg_len;
        if rom_size == 0 { return 0; }
        (bank_num * bank_size) % rom_size
    }

    fn get_prg_mapped_addr(&self, addr: u16) -> (bool, usize) {
        let (bank_reg, base_bank_align, sub, is_5117) = match self.prg_mode {
            0 => {
                (self.prg_bank[3], 0xFC, addr & 0x7FFF, true)
            }
            1 => {
                if addr < 0xC000 {
                    (self.prg_bank[1], 0xFE, addr & 0x3FFF, false)
                } else {
                    (self.prg_bank[3], 0xFE, addr & 0x3FFF, true)
                }
            }
            2 => {
                if addr < 0xC000 {
                    (self.prg_bank[1], 0xFE, addr & 0x3FFF, false)
                } else if addr < 0xE000 {
                    (self.prg_bank[2], 0xFF, addr & 0x1FFF, false)
                } else {
                    (self.prg_bank[3], 0xFF, addr & 0x1FFF, true)
                }
            }
            _ => {
                let bank_idx = ((addr - 0x8000) >> 13) as usize;
                (self.prg_bank[bank_idx], 0xFF, addr & 0x1FFF, bank_idx == 3)
            }
        };

        let is_rom = (bank_reg & 0x80) != 0 || is_5117;
        let bank = (bank_reg as usize) & 0x7F & base_bank_align;
        let offset = (bank * 0x2000) + sub as usize;

        (is_rom, offset)
    }

    fn chr_bank_offset(&self, bank: u8) -> usize {
        let chr_size = self.chr_len;
        if chr_size == 0 { return 0; }
        match self.chr_mode {
            0 => {
                let b = ((self.chr_upper_bank as usize & 0x03) << 7) | (bank as usize & 0x7F);
                (b * 0x2000) % chr_size
            }
            1 => {
                let b = ((self.chr_upper_bank as usize & 0x03) << 8) | (bank as usize);
                (b * 0x1000) % chr_size
            }
            2 => {
                let b = ((self.chr_upper_bank as usize & 0x03) << 8) | (bank as usize);
                (b * 0x800) % chr_size
            }
            _ => {
                let b = ((self.chr_upper_bank as usize & 0x03) << 8) | (bank as usize);
                (b * 0x400) % chr_size
            }
        }
    }

    fn get_nt_nametable_index(addr: u16) -> usize {
        ((addr & 0x0FFF) >> 10) as usize
    }

    fn read_nametable(&self, addr: u16) -> u8 {
        let nt_idx = Self::get_nt_nametable_index(addr);
        let slot_mode = (self.nametable_mode >> (nt_idx * 2)) & 3;
        let offset = (addr & 0x03FF) as usize;

        match slot_mode {
            0 => self.vram[offset],
            1 => self.vram[0x400 + offset],
            2 => self.exram[offset],
            3 => {
                if offset >= 0x3C0 {
                    self.fill_attr
                } else {
                    self.fill_tile
                }
            }
            _ => unreachable!(),
        }
    }

    fn read_attribute(&self, addr: u16) -> u8 {
        let nt_idx = Self::get_nt_nametable_index(addr);
        let slot_mode = (self.nametable_mode >> (nt_idx * 2)) & 3;
        let offset = (addr & 0x0038) as usize;

        match slot_mode {
            0 => self.vram[0x3C0 + offset],
            1 => self.vram[0x400 + 0x3C0 + offset],
            2 => self.exram[0x3C0 + offset],
            3 => self.fill_attr,
            _ => 0,
        }
    }

    fn get_chr_offset(&self, addr: u16, is_bg: bool) -> usize {
        let chr_size = self.chr_len;
        if chr_size == 0 { return 0; }

        if is_bg && self.exram_mode == 1 {
            let tile_index = (((self.exram_latch & 0x3F) as usize) << 8) | (self.nt_latch as usize);
            return (tile_index * 16 + (addr as usize & 0x0F)) % chr_size;
        }

        let banks = if is_bg { &self.chr_bank_nt[..] } else { &self.chr_bank[..] };

        match self.chr_mode {
            0 => {
                let bank = self.chr_bank[7];
                let offset = self.chr_bank_offset(bank);
                (offset + (addr as usize)) % chr_size
            }
            1 => {
                let bank = if is_bg {
                    banks[3]
                } else {
                    if addr < 0x1000 { banks[5] } else { banks[7] }
                };
                let offset = self.chr_bank_offset(bank);
                (offset + ((addr as usize) % 0x1000)) % chr_size
            }
            2 => {
                let bank = if is_bg {
                    if (addr & 0x0FFF) < 0x0800 { banks[1] } else { banks[3] }
                } else {
                    match addr {
                        0x0000..=0x07FF | 0x0800..=0x0FFF => banks[3],
                        0x1000..=0x17FF | 0x1800..=0x1FFF => banks[5],
                        _ => banks[7],
                    }
                };
                let offset = self.chr_bank_offset(bank);
                (offset + ((addr as usize) % 0x0800)) % chr_size
            }
            _ => {
                let bank_index = if is_bg {
                    ((addr & 0x0FFF) >> 10) as usize
                } else {
                    (addr >> 10) as usize
                };
                let bank = banks[bank_index];
                let offset = self.chr_bank_offset(bank);
                (offset + ((addr as usize) & 0x3FF)) % chr_size
            }
        }
    }
}

impl Mapper for MapperMmc5 {
    fn peek(&mut self, addr: u16) -> u8 {
        // --- HARDWARE VBLANK INTERCEPT ---
        if addr == 0xFFFA || addr == 0xFFFB {
            //if self.in_frame { eprintln!("[MMC5-DEBUG] CPU read NMI vector ({:04X}). in_frame = false.", addr); }
            self.in_frame = false;
            self.last_ppu_address = 0xFFFF;
        }

        // --- CYCLE & SCANLINE TRACKING ---
if addr < 0x4000 {
            self.cpu_reads_since_ppu = 0;
            
            if addr < 0x2000 {
                // Increment our count of consecutive Pattern Table reads
                self.ppu_match_count = self.ppu_match_count.saturating_add(1);
            } else if (0x2000..=0x2FFF).contains(&addr) {
                // Background tile fetches only ever do 2 PT reads in a row.
                // Sprite fetches do 16 PT reads in a row. 
                // If we see >= 3 PT reads followed by a Nametable read, 
                // we've hit tick 321: the start of a new scanline!
                if self.ppu_match_count >= 3 {
                    if !self.in_frame {
                        //eprintln!("[MMC5-DEBUG] Hardware Scanline Triggered! Frame Started (in_frame = true)");
                        self.in_frame = true;
                        self.irq_counter = 0;
                    } else {
                        self.irq_counter = self.irq_counter.wrapping_add(1);
                        //eprintln!("[MMC5-DEBUG] Scanline counter incremented to {}", self.irq_counter);
                        
                        // Latch $00 never triggers an IRQ
                        if self.irq_latch != 0 && self.irq_counter == self.irq_latch {
                            self.irq_pending = true; 
                            //eprintln!("[MMC5-DEBUG] IRQ PENDING SET! Matched latch value {}", self.irq_latch);
                        }
                    }
                }
                // Reset the PT read counter now that we're reading Nametables
                self.ppu_match_count = 0;
            } else {
                self.ppu_match_count = 0;
            }
            
            self.last_ppu_address = addr; // Keep this to prevent save state struct breakage
        } else {
            // Instruction batching fallback
            self.cpu_reads_since_ppu = self.cpu_reads_since_ppu.saturating_add(1);
            if self.cpu_reads_since_ppu > 200 {
                self.in_frame = false;
            }
        }
        // ---------------------------------

        match addr {
            0x0000..=0x1FFF => {
                if self.chr_len == 0 { return 0; }
                let is_bg = self.bg_chr_fetches_left > 0;
                if is_bg { self.bg_chr_fetches_left -= 1; }
                let offset = self.get_chr_offset(addr, is_bg);
                self.cart.chr_rom[offset]
            }
            0x2000..=0x2EFF | 0x2F00..=0x3EFF => {
                let is_attr = (addr & 0x03FF) >= 0x03C0;
                if is_attr {
                    self.bg_chr_fetches_left = 2; 
                    if self.exram_mode == 1 {
                        let pal = (self.exram_latch >> 6) & 3;
                        return pal | (pal << 2) | (pal << 4) | (pal << 6);
                    } else {
                        return self.read_attribute(addr);
                    }
                } else {
                    let nt_data = self.read_nametable(addr);
                    self.nt_latch = nt_data;
                    let offset = (addr & 0x03FF) as usize;
                    self.exram_latch = self.exram[offset];
                    return nt_data;
                }
            }
            0x3F00..=0x3FFF => 0,
            0x5004..=0x5007 => 0,
            0x5100..=0x5117 => 0,
            0x5120..=0x512B => 0,
            0x5130 => 0,

            0x5204 => {
                let mut val = 0x00;
                if self.irq_pending { val |= 0x80; }
                if self.in_frame { val |= 0x40; }
                
               // eprintln!("[MMC5-DEBUG] CPU Read $5204 -> {:02X} (Pending: {}, InFrame: {})", val, self.irq_pending, self.in_frame);
                
                self.irq_pending = false; // Reading $5204 clears the pending flag
                val
            }
            0x5205 => {
                let result = (self.mul_a as u16).wrapping_mul(self.mul_b as u16);
                (result & 0xFF) as u8
            }
            0x5206 => {
                let result = (self.mul_a as u16).wrapping_mul(self.mul_b as u16);
                ((result >> 8) & 0xFF) as u8
            }
            0x5C00..=0x5FFF => {
                match self.exram_mode {
                    0 | 1 | 2 | 3 => self.exram[(addr & 0x03FF) as usize],
                    _ => 0,
                }
            }
            0x6000..=0x7FFF => {
                let bank = self.wram_bank as usize;
                let offset = (bank * 0x2000) + (addr & 0x1FFF) as usize;
                if offset < self.wram.len() {
                    self.wram[offset]
                } else {
                    0
                }
            }
            0x8000..=0xFFFF => {
                let (is_rom, offset) = self.get_prg_mapped_addr(addr);
                if is_rom {
                    if self.prg_len == 0 { return 0; }
                    self.cart.prg_rom[offset % self.prg_len]
                } else {
                    let wram_len = self.wram.len();
                    if wram_len == 0 { return 0; }
                    self.wram[offset % wram_len]
                }
            }
            _ => 0,
        }
    }

    fn poke(&mut self, addr: u16, val: u8) {
        // --- MANUAL RENDER DISABLE INTERCEPT ---
        if addr == 0x2001 {
            if (val & 0x18) == 0 { 
              //  if self.in_frame { eprintln!("[MMC5-DEBUG] CPU wrote to $2001 disabling rendering. in_frame = false."); }
                self.in_frame = false;
                self.last_ppu_address = 0xFFFF;
            }
        }

        if addr < 0x4000 {
            self.cpu_reads_since_ppu = 0;
        } else {
            self.cpu_reads_since_ppu = self.cpu_reads_since_ppu.saturating_add(1);
            if self.cpu_reads_since_ppu > 200 {
               // if self.in_frame { eprintln!("[MMC5-DEBUG] 200 CPU cycles passed without PPU read. in_frame = false."); }
                self.in_frame = false;
            }
        }

        match addr {
            0x0000..=0x1FFF => {
                if self.chr_len == 0 { return; }
                let offset = self.get_chr_offset(addr, false);
                if offset < self.chr_len {
                    self.cart.chr_rom[offset] = val;
                }
            }
            0x2000..=0x3EFF => {
                let nt_idx = Self::get_nt_nametable_index(addr);
                let slot_mode = (self.nametable_mode >> (nt_idx * 2)) & 3;
                let offset = (addr & 0x03FF) as usize;

                match slot_mode {
                    0 => { self.vram[offset] = val; }
                    1 => { self.vram[0x400 + offset] = val; }
                    2 => {
                        if self.exram_mode != 3 { self.exram[offset] = val; }
                    }
                    3 => {}
                    _ => {}
                }
            }
            0x3F00..=0x3FFF => {}

            0x5100 => self.prg_mode = val & 3,
            0x5101 => self.chr_mode = val & 3,
            0x5102 => self.prg_ram_write_1 = val & 3,
            0x5103 => self.prg_ram_write_2 = val & 3,
            0x5104 => self.exram_mode = val & 3,
            0x5105 => {
                self.nametable_mode = val;
                let nt0 = val & 3;
                let nt1 = (val >> 2) & 3;
                if nt0 == 0 && nt1 == 1 {
                    self.mirror_mode = MirrorMode::MirrorHorizontal;
                } else if nt0 == 1 && nt1 == 0 {
                    self.mirror_mode = MirrorMode::MirrorVertical;
                } else if nt0 == 1 && nt1 == 1 {
                    self.mirror_mode = MirrorMode::MirrorHorizontal;
                } else {
                    self.mirror_mode = MirrorMode::MirrorHorizontal;
                }
            }
            0x5106 => self.fill_tile = val,
            0x5107 => self.fill_attr = val & 3,

            0x5113 => self.wram_bank = val & 0x07,

            0x5114 => self.prg_bank[0] = val,
            0x5115 => self.prg_bank[1] = val,
            0x5116 => self.prg_bank[2] = val,
            0x5117 => self.prg_bank[3] = val,

            0x5120 => self.chr_bank[0] = val,
            0x5121 => self.chr_bank[1] = val,
            0x5122 => self.chr_bank[2] = val,
            0x5123 => self.chr_bank[3] = val,
            0x5124 => self.chr_bank[4] = val,
            0x5125 => self.chr_bank[5] = val,
            0x5126 => self.chr_bank[6] = val,
            0x5127 => self.chr_bank[7] = val,
            0x5128 => self.chr_bank_nt[0] = val,
            0x5129 => self.chr_bank_nt[1] = val,
            0x512A => self.chr_bank_nt[2] = val,
            0x512B => self.chr_bank_nt[3] = val,

            0x5130 => self.chr_upper_bank = val & 3,

            0x5203 => {
                //eprintln!("[MMC5-DEBUG] CPU wrote IRQ latch: {}", val);
                self.irq_latch = val;
            }
            0x5204 => {
                self.irq_enabled = (val & 0x80) != 0;
                //eprintln!("[MMC5-DEBUG] CPU wrote $5204. IRQ Enabled = {}", self.irq_enabled);
            }

            0x5205 => self.mul_a = val,
            0x5206 => self.mul_b = val,

            0x6000..=0x7FFF => {
                let w1 = self.prg_ram_write_1;
                let w2 = self.prg_ram_write_2;
                if (w1 & 0x03) == 0x02 && (w2 & 0x03) == 0x01 {
                    let bank = self.wram_bank as usize;
                    let offset = (bank * 0x2000) + (addr & 0x1FFF) as usize;
                    if offset < self.wram.len() {
                        self.wram[offset] = val;
                    }
                }
            }

            0x5C00..=0x5FFF => {
                match self.exram_mode {
                    0 | 1 | 2 => {
                        self.exram[(addr & 0x03FF) as usize] = val;
                    }
                    3 => {} 
                    _ => {}
                }
            }

            0x8000..=0xFFFF => {
                let w1 = self.prg_ram_write_1;
                let w2 = self.prg_ram_write_2;
                if (w1 & 0x03) == 0x02 && (w2 & 0x03) == 0x01 {
                    let (is_rom, offset) = self.get_prg_mapped_addr(addr);
                    if !is_rom {
                        let wram_len = self.wram.len();
                        if wram_len > 0 {
                            self.wram[offset % wram_len] = val;
                        }
                    }
                }
            }

            _ => {}
        }
    }

    #[inline]
    fn check_irq(&self) -> bool {
        // Must check BOTH flags before firing the line to the CPU
        self.irq_pending && self.irq_enabled
    }

    fn get_id(&self) -> u8 {
        Self::ID
    }

    fn update_cartridge(&mut self, cartridge: Cartridge) {
        self.prg_len = cartridge.prg_rom.len();
        self.chr_len = cartridge.chr_rom.len();
        self.cart = cartridge;
    }

    fn get_prg_rom_offset(&self, addr: u16) -> Option<u32> {
        match addr {
            0x8000..=0xFFFF => {
                let (is_rom, offset) = self.get_prg_mapped_addr(addr);
                if is_rom {
                    if self.prg_len == 0 {
                        None
                    } else {
                        Some((offset % self.prg_len) as u32)
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn get_sram(&self) -> Option<&[u8]> {
        Some(&self.wram)
    }

    fn set_sram(&mut self, data: &[u8]) {
        let len = usize::min(data.len(), self.wram.len());
        self.wram[..len].copy_from_slice(&data[..len]);
    }

    fn write_state_to(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.prg_mode.write(writer)?;
        self.prg_bank.write(writer)?;
        self.chr_mode.write(writer)?;
        self.chr_upper_bank.write(writer)?;
        self.chr_bank.write(writer)?;
        self.chr_bank_nt.write(writer)?;
        self.nametable_mode.write(writer)?;
        self.vram.write(writer)?;
        self.fill_tile.write(writer)?;
        self.fill_attr.write(writer)?;
        self.exram_mode.write(writer)?;
        self.exram.write(writer)?;
        self.irq_enabled.write(writer)?;
        self.irq_pending.write(writer)?;
        self.irq_latch.write(writer)?;
        self.irq_counter.write(writer)?;
        self.in_frame.write(writer)?;
        self.mul_a.write(writer)?;
        self.mul_b.write(writer)?;
        self.wram.write(writer)?;
        self.wram_enabled.write(writer)?;
        self.wram_bank.write(writer)?;
        self.prg_ram_write_1.write(writer)?;
        self.prg_ram_write_2.write(writer)?;
        self.mirror_mode.write(writer)?;
        
        self.nt_latch.write(writer)?;
        self.exram_latch.write(writer)?;
        self.bg_chr_fetches_left.write(writer)?;
        
        self.last_ppu_address.write(writer)?;
        self.ppu_match_count.write(writer)
    }

    fn read_state_from(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.prg_mode = ReadState::read(reader)?;
        self.prg_bank = ReadState::read(reader)?;
        self.chr_mode = ReadState::read(reader)?;
        self.chr_upper_bank = ReadState::read(reader)?;
        self.chr_bank = ReadState::read(reader)?;
        self.chr_bank_nt = ReadState::read(reader)?;
        self.nametable_mode = ReadState::read(reader)?;
        self.vram = ReadState::read(reader)?;
        self.fill_tile = ReadState::read(reader)?;
        self.fill_attr = ReadState::read(reader)?;
        self.exram_mode = ReadState::read(reader)?;
        self.exram = ReadState::read(reader)?;
        self.irq_enabled = ReadState::read(reader)?;
        self.irq_pending = ReadState::read(reader)?;
        self.irq_latch = ReadState::read(reader)?;
        self.irq_counter = ReadState::read(reader)?;
        self.in_frame = ReadState::read(reader)?;
        self.mul_a = ReadState::read(reader)?;
        self.mul_b = ReadState::read(reader)?;
        self.wram = ReadState::read(reader)?;
        self.wram_enabled = ReadState::read(reader)?;
        self.prg_ram_write_1 = ReadState::read(reader)?;
        self.prg_ram_write_2 = ReadState::read(reader)?;
        self.mirror_mode = ReadState::read(reader)?;
        
        self.nt_latch = ReadState::read(reader)?;
        self.exram_latch = ReadState::read(reader)?;
        self.bg_chr_fetches_left = ReadState::read(reader)?;
        
        self.last_ppu_address = ReadState::read(reader)?;
        self.ppu_match_count = ReadState::read(reader)?;
        self.prg_len = self.cart.prg_rom.len();
        self.chr_len = self.cart.chr_rom.len();
        Ok(())
    }
}