use std::io::{self, Read, Write};

use crate::impl_write_state_generic_array;

use super::cpu;
use super::nes::{State, FRAME_DEPTH, FRAME_SIZE, FRAME_WIDTH};
use super::save_state::{ReadState, WriteState};

const COLORS: [u32; 64] = [
    0x545454, 0x001e74, 0x081090, 0x300088, 0x440064, 0x5c0030, 0x540400, 0x3c1800, 0x202a00,
    0x083a00, 0x004000, 0x003c00, 0x00323c, 0x000000, 0x000000, 0x000000, 0x989698, 0x084cc4,
    0x3032ec, 0x5c1ee4, 0x8814b0, 0xa01464, 0x982220, 0x783c00, 0x545a00, 0x287200, 0x087c00,
    0x007628, 0x006678, 0x000000, 0x000000, 0x000000, 0xeceeec, 0x4c9aec, 0x787cec, 0xb062ec,
    0xe454ec, 0xec58b4, 0xec6a64, 0xd48820, 0xa0aa00, 0x74c400, 0x4cd020, 0x38cc6c, 0x38b4cc,
    0x3c3c3c, 0x000000, 0x000000, 0xeceeec, 0xa8ccec, 0xbcbcec, 0xd4b2ec, 0xecaeec, 0xecaed4,
    0xecb4b0, 0xe4c490, 0xccd278, 0xb4de78, 0xa8e290, 0x98e2b4, 0xa0d6e4, 0xa0a2a0, 0x000000,
    0x000000,
];

#[inline]
pub fn palette_to_rgba(data: u8) -> u32 {
    let col = COLORS[(data & 0x3F) as usize];
    u32::from_le_bytes([(col >> 16) as u8, (col >> 8) as u8, col as u8, 255])
}

#[derive(Copy, Clone)]
struct SpriteBufferData {
    id: u8,
    color: u8,
    priority: bool,
    sprite0: bool,
    generation: u32,
}

impl Default for SpriteBufferData {
    #[inline]
    fn default() -> Self {
        SpriteBufferData {
            id: 0xFF,
            color: 0,
            priority: false,
            sprite0: false,
            generation: 0,
        }
    }
}

impl WriteState for SpriteBufferData {
    #[inline]
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.id.write(writer)?;
        self.color.write(writer)?;
        self.priority.write(writer)?;
        self.sprite0.write(writer)
    }
}

impl ReadState for SpriteBufferData {
    #[inline]
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        Ok(SpriteBufferData {
            id: ReadState::read(reader)?,
            color: ReadState::read(reader)?,
            priority: ReadState::read(reader)?,
            sprite0: ReadState::read(reader)?,
            generation: 0,
        })
    }
}

#[allow(dead_code)]
pub struct PpuState {
    pub scanline: u16,
    pub tick: u16,
    pub frames: u64,
    cycles: u64,

    // Determines if we batch scanlines or do cycle-accurate emulation for complex mappers
    pub precise_timing: bool,

    // Last CPU cycle that we emulated at.
    last_cpu_cycle: u64,

    pub frame_buffer: Vec<u8>,

    is_rendering: bool,
    data_buffer: u8,
    latch: u8,
    sprite_overflow: u8,
    sprite0_hit: bool,
    vblank: u8,

    oam_addr: usize,
    pub oam_1: [u8; 256],
    oam_2: [u8; 32],
    sprite_eval_n: usize,
    sprite_eval_m: usize,
    sprite_eval_read: u8,
    sprite_eval_scanline_count: usize,
    sprite_eval_has_sprite0: bool,
    sprite_buffer: [SpriteBufferData; 256],
    sprite_buffer_generation: u32,

    pub palette: [u8; 32],
    pub palette_rgba: [u32; 32],
    bg_data: [u8; 24],
    bg_data_index: usize,

    // Scrolling registers
    v: u16,
    t: u16,
    x: u16,
    w: u8,

    // PPUCTRL
    flag_vram_increment: u8,
    flag_sprite_table_addr: u8,
    flag_background_table_addr: u8,
    pub flag_sprite_size: u8,
    flag_master_slave: u8,
    flag_generate_nmi: bool,

    // PPUMASK
    flag_grayscale: bool,
    flag_show_sprites_left: bool,
    flag_show_background_left: bool,
    flag_render_sprites: bool,
    flag_render_background: bool,
    flag_emphasize_red: bool,
    flag_emphasize_green: bool,
    flag_emphasize_blue: bool,
}

impl_write_state_generic_array!(SpriteBufferData, 256);

impl WriteState for PpuState {
    #[inline]
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.scanline.write(writer)?;
        self.tick.write(writer)?;
        self.frames.write(writer)?;
        self.cycles.write(writer)?;
        self.last_cpu_cycle.write(writer)?;
        self.frame_buffer.write(writer)?;
        self.is_rendering.write(writer)?;
        self.data_buffer.write(writer)?;
        self.latch.write(writer)?;
        self.sprite_overflow.write(writer)?;
        self.sprite0_hit.write(writer)?;
        self.vblank.write(writer)?;
        self.oam_addr.write(writer)?;
        self.oam_1.write(writer)?;
        self.oam_2.write(writer)?;
        self.sprite_eval_n.write(writer)?;
        self.sprite_eval_m.write(writer)?;
        self.sprite_eval_read.write(writer)?;
        self.sprite_eval_scanline_count.write(writer)?;
        self.sprite_eval_has_sprite0.write(writer)?;
        self.sprite_buffer.write(writer)?;
        self.palette.write(writer)?;
        self.bg_data.write(writer)?;
        self.bg_data_index.write(writer)?;
        self.v.write(writer)?;
        self.t.write(writer)?;
        self.x.write(writer)?;
        self.w.write(writer)?;
        self.flag_vram_increment.write(writer)?;
        self.flag_sprite_table_addr.write(writer)?;
        self.flag_background_table_addr.write(writer)?;
        self.flag_sprite_size.write(writer)?;
        self.flag_master_slave.write(writer)?;
        self.flag_generate_nmi.write(writer)?;
        self.flag_grayscale.write(writer)?;
        self.flag_show_sprites_left.write(writer)?;
        self.flag_show_background_left.write(writer)?;
        self.flag_render_sprites.write(writer)?;
        self.flag_render_background.write(writer)?;
        self.flag_emphasize_red.write(writer)?;
        self.flag_emphasize_green.write(writer)?;
        self.flag_emphasize_blue.write(writer)?;
        self.sprite_buffer_generation.write(writer)?;
        self.precise_timing.write(writer)
    }
}

impl ReadState for PpuState {
    #[inline]
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        let mut state = PpuState {
            scanline: ReadState::read(reader)?,
            tick: ReadState::read(reader)?,
            frames: ReadState::read(reader)?,
            cycles: ReadState::read(reader)?,
            last_cpu_cycle: ReadState::read(reader)?,
            frame_buffer: ReadState::read(reader)?,
            is_rendering: ReadState::read(reader)?,
            data_buffer: ReadState::read(reader)?,
            latch: ReadState::read(reader)?,
            sprite_overflow: ReadState::read(reader)?,
            sprite0_hit: ReadState::read(reader)?,
            vblank: ReadState::read(reader)?,
            oam_addr: ReadState::read(reader)?,
            oam_1: ReadState::read(reader)?,
            oam_2: ReadState::read(reader)?,
            sprite_eval_n: ReadState::read(reader)?,
            sprite_eval_m: ReadState::read(reader)?,
            sprite_eval_read: ReadState::read(reader)?,
            sprite_eval_scanline_count: ReadState::read(reader)?,
            sprite_eval_has_sprite0: ReadState::read(reader)?,
            sprite_buffer: ReadState::read(reader)?,
            palette: ReadState::read(reader)?,
            palette_rgba: [0; 32],
            bg_data: ReadState::read(reader)?,
            bg_data_index: ReadState::read(reader)?,
            v: ReadState::read(reader)?,
            t: ReadState::read(reader)?,
            x: ReadState::read(reader)?,
            w: ReadState::read(reader)?,
            flag_vram_increment: ReadState::read(reader)?,
            flag_sprite_table_addr: ReadState::read(reader)?,
            flag_background_table_addr: ReadState::read(reader)?,
            flag_sprite_size: ReadState::read(reader)?,
            flag_master_slave: ReadState::read(reader)?,
            flag_generate_nmi: ReadState::read(reader)?,
            flag_grayscale: ReadState::read(reader)?,
            flag_show_sprites_left: ReadState::read(reader)?,
            flag_show_background_left: ReadState::read(reader)?,
            flag_render_sprites: ReadState::read(reader)?,
            flag_render_background: ReadState::read(reader)?,
            flag_emphasize_red: ReadState::read(reader)?,
            flag_emphasize_green: ReadState::read(reader)?,
            flag_emphasize_blue: ReadState::read(reader)?,
            sprite_buffer_generation: ReadState::read(reader).unwrap_or(0),
            precise_timing: ReadState::read(reader).unwrap_or(false),
        };
        state.update_palette_cache();
        Ok(state)
    }
}

impl PpuState {
    #[inline]
    pub fn new() -> PpuState {
        let mut s = PpuState {
            scanline: 0,
            tick: 0,
            frames: 0,
            cycles: 0,
            precise_timing: false,
            last_cpu_cycle: 7,
            frame_buffer: vec![0; FRAME_SIZE],
            is_rendering: false,
            data_buffer: 0,
            latch: 0,
            sprite_overflow: 0,
            sprite0_hit: false,
            vblank: 0,
            oam_addr: 0,
            oam_1: [0; 256],
            oam_2: [0; 32],
            sprite_eval_n: 0,
            sprite_eval_m: 0,
            sprite_eval_read: 0,
            sprite_eval_scanline_count: 0,
            sprite_eval_has_sprite0: false,
            sprite_buffer: [SpriteBufferData::default(); 256],
            sprite_buffer_generation: 0,
            palette: [0; 32],
            palette_rgba: [0; 32],
            bg_data: [0; 24],
            bg_data_index: 0,
            v: 0,
            t: 0,
            x: 0,
            w: 0,
            flag_vram_increment: 0,
            flag_sprite_table_addr: 0,
            flag_background_table_addr: 0,
            flag_sprite_size: 0,
            flag_master_slave: 0,
            flag_generate_nmi: false,
            flag_grayscale: false,
            flag_show_sprites_left: false,
            flag_show_background_left: false,
            flag_render_sprites: false,
            flag_render_background: false,
            flag_emphasize_red: false,
            flag_emphasize_green: false,
            flag_emphasize_blue: false,
        };
        s.update_palette_cache();
        s
    }

    fn update_palette_cache(&mut self) {
        for i in 0..32 {
            self.palette_rgba[i] = palette_to_rgba(self.palette[i]);
        }
    }
}

#[inline]
pub fn catch_up(s: &mut State) {
    let cpu_cycles = s.cpu.cycles - s.ppu.last_cpu_cycle;
    if cpu_cycles == 0 {
        return;
    }
    
    if s.ppu.precise_timing {
        emulate_precise(s, cpu_cycles * 3);
    } else {
        emulate_batched(s, cpu_cycles * 3);
    }
}

#[inline]
fn emulate_precise(s: &mut State, cycles: u64) {
    s.ppu.last_cpu_cycle = s.cpu.cycles;

    let mut cycles_left = cycles;
    while cycles_left > 0 {
        let rendering_enabled = s.ppu.flag_render_sprites || s.ppu.flag_render_background;
        let scanline = s.ppu.scanline;
        let tick = s.ppu.tick;

        if scanline == 261 {
            if tick == 1 {
                s.ppu.sprite0_hit = false;
                s.ppu.vblank = 0;
                s.ppu.is_rendering = true;
            } else if tick == 304 && rendering_enabled {
                s.ppu.v = (s.ppu.v & 0x841F) | (s.ppu.t & 0x7BE0);
            }
        }

        let is_active_scanline = scanline < 240 || scanline == 261;
        
        if is_active_scanline && rendering_enabled {
            if (tick >= 1 && tick <= 256) || (tick >= 321 && tick <= 336) {
                let next = s.ppu.bg_data_index + 1;
                s.ppu.bg_data_index = if next == 24 { 0 } else { next };

                if tick & 0x7 == 1 {
                    fetch_tile(s);
                }
            }

            if scanline < 240 && tick >= 1 && tick <= 256 {
                if scanline < 8 || scanline >= 232 {
                    if tick == 1 {
                        let y = scanline as usize;
                        let row_start = (y * FRAME_WIDTH) * FRAME_DEPTH;
                        let row_end = row_start + FRAME_WIDTH * FRAME_DEPTH;
                        let frame = &mut s.ppu.frame_buffer;
                        let mut i = row_start;
                        while i < row_end {
                            frame[i] = 0;
                            frame[i + 1] = 0;
                            frame[i + 2] = 0;
                            frame[i + 3] = 255;
                            i += FRAME_DEPTH;
                        }
                    }
                } else {
                    render_pixel(s);
                }
            }

            sprite_evaluation(s);

            if tick == 256 {
                increment_scroll_y(&mut s.ppu);
            } else if tick == 257 {
                s.ppu.v = (s.ppu.v & 0xFBE0) | (s.ppu.t & 0x41F);
            } else if ((tick >= 321 && tick <= 336) || (tick >= 1 && tick <= 256)) && (tick & 0x7 == 0) {
                increment_scroll_x(&mut s.ppu);
            }
        }

        if scanline == 241 && tick == 1 {
            if s.ppu.flag_generate_nmi {
                s.cpu.pending_interrupt = cpu::InterruptKind::NMI;
            }
            s.ppu.is_rendering = false;
            s.ppu.vblank = 1;
            s.ppu.frames += 1;
        }

        s.ppu.cycles += 1;
        s.ppu.tick += 1;
        
        if scanline == 261 && (s.ppu.frames & 1 != 0) && s.ppu.tick == 340 && rendering_enabled {
            s.ppu.tick += 1;
        }
        
        if s.ppu.tick >= 341 {
            s.ppu.tick = 0;
            s.ppu.scanline += 1;
            if s.ppu.scanline > 261 {
                s.ppu.scanline = 0;
            }
        }
        
        cycles_left -= 1;
    }
}

#[inline]
fn emulate_batched(s: &mut State, mut cycles_left: u64) {
    s.ppu.last_cpu_cycle = s.cpu.cycles;

    while cycles_left > 0 {
        let rendering_enabled = s.ppu.flag_render_sprites || s.ppu.flag_render_background;

        // Fast path: Non-visible scanlines (VBlank and Overscan)
        if s.ppu.scanline >= 240 && s.ppu.scanline < 261 {
            // Ensure we catch the exact NMI trigger tick
            if s.ppu.scanline == 241 && s.ppu.tick == 0 && cycles_left > 0 {
                s.ppu.tick = 1;
                cycles_left -= 1;
                s.ppu.cycles += 1;
                
                if s.ppu.flag_generate_nmi {
                    s.cpu.pending_interrupt = cpu::InterruptKind::NMI;
                }
                s.ppu.is_rendering = false;
                s.ppu.vblank = 1;
                s.ppu.frames += 1;
                continue;
            }

            // Fast forward to the end of the scanline
            let ticks_to_end = 341 - s.ppu.tick;
            let step = std::cmp::min(cycles_left, ticks_to_end as u64) as u16;
            
            s.ppu.tick += step;
            s.ppu.cycles += step as u64;
            cycles_left -= step as u64;

            if s.ppu.tick >= 341 {
                s.ppu.tick = 0;
                s.ppu.scanline += 1;
            }
            continue;
        }

        // Fast path: Active scanlines executed in a tight batch
        let ticks_to_end = 341 - s.ppu.tick;
        if cycles_left >= ticks_to_end as u64 {
            // Finish this scanline in one tight loop
            run_batched_active_scanline(s, s.ppu.tick, 341, rendering_enabled);
            
            s.ppu.cycles += ticks_to_end as u64;
            cycles_left -= ticks_to_end as u64;
            
            s.ppu.tick = 0;
            s.ppu.scanline += 1;
            if s.ppu.scanline > 261 {
                s.ppu.scanline = 0;
            }
        } else {
            // Partially finish the scanline
            let end_tick = s.ppu.tick + cycles_left as u16;
            run_batched_active_scanline(s, s.ppu.tick, end_tick, rendering_enabled);
            
            s.ppu.tick = end_tick;
            s.ppu.cycles += cycles_left;
            cycles_left = 0;
        }
    }
}

#[inline]
fn run_batched_active_scanline(s: &mut State, start_tick: u16, end_tick: u16, rendering_enabled: bool) {
    let scanline = s.ppu.scanline;
    let is_active_scanline = scanline < 240 || scanline == 261;

    for tick in start_tick..end_tick {
        s.ppu.tick = tick; 

        if scanline == 261 {
            if tick == 1 {
                s.ppu.sprite0_hit = false;
                s.ppu.vblank = 0;
                s.ppu.is_rendering = true;
            } else if tick == 304 && rendering_enabled {
                s.ppu.v = (s.ppu.v & 0x841F) | (s.ppu.t & 0x7BE0);
            }
        }

        if is_active_scanline && rendering_enabled {
            if (tick >= 1 && tick <= 256) || (tick >= 321 && tick <= 336) {
                let next = s.ppu.bg_data_index + 1;
                s.ppu.bg_data_index = if next == 24 { 0 } else { next };

                if tick & 0x7 == 1 {
                    fetch_tile(s);
                }
            }

            if scanline < 240 && tick >= 1 && tick <= 256 {
                if scanline < 8 || scanline >= 232 {
                    if tick == 1 {
                        let y = scanline as usize;
                        let row_start = (y * FRAME_WIDTH) * FRAME_DEPTH;
                        let row_end = row_start + FRAME_WIDTH * FRAME_DEPTH;
                        let frame = &mut s.ppu.frame_buffer;
                        let mut i = row_start;
                        while i < row_end {
                            frame[i] = 0;
                            frame[i + 1] = 0;
                            frame[i + 2] = 0;
                            frame[i + 3] = 255;
                            i += FRAME_DEPTH;
                        }
                    }
                } else {
                    render_pixel(s);
                }
            }

            sprite_evaluation(s);

            if tick == 256 {
                increment_scroll_y(&mut s.ppu);
            } else if tick == 257 {
                s.ppu.v = (s.ppu.v & 0xFBE0) | (s.ppu.t & 0x41F);
            } else if ((tick >= 321 && tick <= 336) || (tick >= 1 && tick <= 256)) && (tick & 0x7 == 0) {
                increment_scroll_x(&mut s.ppu);
            }
        }

        if scanline == 261 && (s.ppu.frames & 1 != 0) && tick == 340 && rendering_enabled {
            s.ppu.tick += 1;
            // The odd frame skip logic skips the cycle in precise rendering, handled gracefully here
        }
    }
}

#[inline]
fn sprite_evaluation(s: &mut State) {
    let tick = s.ppu.tick;
    if tick != 1 && tick != 256 && (tick < 65 || tick > 320 || (tick > 256 && (tick & 7) != 0)) {
        return;
    }
    match tick {

        1 => {
            s.ppu.oam_2.fill(0xFF);

            s.ppu.sprite_eval_n = 0;
            s.ppu.sprite_eval_m = 0;
            s.ppu.sprite_eval_scanline_count = 0;
            s.ppu.sprite_eval_has_sprite0 = false;
        }
        65..=256 if (s.ppu.sprite_eval_n < 64 && s.ppu.sprite_eval_scanline_count < 8) => {
            if s.ppu.tick & 0x1 == 1 {
                let index = s.ppu.sprite_eval_n * 4 + s.ppu.sprite_eval_m;
                s.ppu.sprite_eval_read = s.ppu.oam_1[index];
            } else {
                let oam_2_addr = 4 * s.ppu.sprite_eval_scanline_count + s.ppu.sprite_eval_m;
                s.ppu.oam_2[oam_2_addr] = s.ppu.sprite_eval_read;
                match s.ppu.sprite_eval_m {
                    0 => {
                        let sprite_height = (8 << s.ppu.flag_sprite_size) as u16;
                        let top = s.ppu.sprite_eval_read as u16;
                        let bottom = top + sprite_height;
                        if s.ppu.scanline >= top && s.ppu.scanline < bottom {
                            s.ppu.sprite_eval_m += 1;
                        } else {
                            s.ppu.sprite_eval_n += 1;
                        }
                    }
                    3 => {
                        if s.ppu.sprite_eval_n == 0 {
                            s.ppu.sprite_eval_has_sprite0 = true;
                        }
                        s.ppu.sprite_eval_n += 1;
                        s.ppu.sprite_eval_m = 0;
                        s.ppu.sprite_eval_scanline_count += 1;
                    }
                    _ => {
                        s.ppu.sprite_eval_m += 1;
                    }
                }
            }
        }
        256 => {
            s.ppu.sprite_buffer_generation = s.ppu.sprite_buffer_generation.wrapping_add(1);
        }
        257..=320 if (s.ppu.tick & 0x7 == 0) => {
            let n = ((s.ppu.tick - 257) / 8) as usize;

            let mut y_pos = s.ppu.oam_2[n * 4 + 0];
            let mut tile = s.ppu.oam_2[n * 4 + 1];
            let mut attribute = s.ppu.oam_2[n * 4 + 2];
            let mut x_pos = s.ppu.oam_2[n * 4 + 3];

            if n >= s.ppu.sprite_eval_scanline_count {
                y_pos = s.ppu.scanline as u8;
                tile = 0xFF;
                attribute = 0xFF;
                x_pos = 0xFF;
            }

            let mut sprite_table = s.ppu.flag_sprite_table_addr;
            let tile_row_raw = s.ppu.scanline.wrapping_sub(y_pos as u16);
            let flip_vertical = attribute & 0x80 > 0;

            if s.ppu.flag_sprite_size > 0 {
                sprite_table = tile & 0x1;
                tile &= 0xFE;

                tile |= ((tile_row_raw >= 8) ^ flip_vertical) as u8;
            }

            let tile_row = if flip_vertical {
                7 - (tile_row_raw & 0x7)
            } else {
                tile_row_raw & 0x7
            };

            let pattern_addr = 0 | tile_row | ((tile as u16) << 4) | ((sprite_table as u16) << 12);
            let lo = s.ppu_peek(pattern_addr);
            let hi = s.ppu_peek(pattern_addr | 0x8);

            if x_pos == 0xFF {
                return;
            }

            let flip_horizontal = attribute & 0x40 > 0;
            for i in 0..8 {
                let x_off = if flip_horizontal { i } else { 7 - i };
                let buf_x = (x_pos as usize) + (x_off as usize);
                if buf_x >= 256 {
                    continue;
                }
                let entry = &mut s.ppu.sprite_buffer[buf_x];
                if entry.generation != s.ppu.sprite_buffer_generation || entry.id == 0xFF || (entry.color & 0b11) == 0 {
                    entry.id = n as u8;
                    entry.generation = s.ppu.sprite_buffer_generation;
                    entry.color = 0b10000
                        | (((lo & (1 << i)) > 0) as u8) << 0
                        | (((hi & (1 << i)) > 0) as u8) << 1
                        | (attribute & 0b11) << 2;
                    entry.priority = (attribute & 0b00100000) == 0;
                    entry.sprite0 = s.ppu.sprite_eval_has_sprite0 && (n == 0);
                }
            }
        }
        _ => {}
    }
}

#[inline(always)]
fn increment_scroll_y(ppu: &mut PpuState) {
    if ppu.v & 0x7000 != 0x7000 {
        ppu.v += 0x1000;
    } else {
        ppu.v &= 0x8FFF;
        let mut y = (ppu.v & 0x03E0) >> 5;
        if y == 29 {
            y = 0;
            ppu.v ^= 0x0800;
        } else if y == 31 {
            y = 0;
        } else {
            y += 1;
        }
        ppu.v = (ppu.v & 0xFC1F) | (y << 5)
    }
}

#[inline(always)]
fn increment_scroll_x(ppu: &mut PpuState) {
    if ppu.v & 0x001F == 31 {
        ppu.v &= 0xFFE0;
        ppu.v ^= 0x0400;
    } else {
        ppu.v += 1;
    }
}

#[inline(always)]
fn render_pixel(s: &mut State) {
    let x = (s.ppu.tick - 1) as usize;
    let y = s.ppu.scanline as usize;

    let mut bg_index = s.ppu.bg_data_index + (s.ppu.x as usize);
    if bg_index >= 24 {
        bg_index -= 24;
    }
    let mut bg_pixel = s.ppu.bg_data[bg_index];
    let sprite_entry = s.ppu.sprite_buffer[x];
    let sprite_gen_valid = sprite_entry.generation == s.ppu.sprite_buffer_generation;
    let mut sprite_pixel = if sprite_gen_valid {
        sprite_entry.color
    } else {
        0
    };

    if x < 8 {
        if !s.ppu.flag_show_background_left {
            bg_pixel = 0;
        }
        if !s.ppu.flag_show_sprites_left {
            sprite_pixel = 0;
        }
    }

    let bg_visible = ((bg_pixel & 0x3) != 0) && s.ppu.flag_render_background;
    let sprite_visible = ((sprite_pixel & 0x3) != 0) && s.ppu.flag_render_sprites;
    let col = match (bg_visible, sprite_visible) {
        (false, false) => 0,
        (true, false) => bg_pixel,
        (false, true) => sprite_pixel,
        (true, true) => {
            if sprite_gen_valid && sprite_entry.sprite0 {
                s.ppu.sprite0_hit = true;
            }
            if sprite_gen_valid && sprite_entry.priority {
                sprite_pixel
            } else {
                bg_pixel
            }
        }
    };

    let pixel = s.ppu.palette_rgba[(col & 0x1F) as usize];
    let frame = s.ppu.frame_buffer.as_mut_ptr();
    let i = (y << 10) | (x << 2);
    unsafe { *(frame.add(i) as *mut u32) = pixel; }
}

#[inline(always)]
fn fetch_tile(s: &mut State) {
    let nt_addr = 0x2000 | (s.ppu.v & 0x0FFF);
    let nt_data = s.ppu_peek(nt_addr) as u16;
    let at_addr = 0x23C0 | (s.ppu.v & 0x0C00) | ((s.ppu.v >> 4) & 0x38) | ((s.ppu.v >> 2) & 0x07);
    let at_data = s.ppu_peek(at_addr) as u16;

    let at_data = ((at_data >> (((s.ppu.v >> 4) & 4) | (s.ppu.v & 2))) & 3) << 2;

    let pattern_addr: u16 = 0
        | ((s.ppu.v >> 12) & 0x7)
        | nt_data << 4
        | (s.ppu.flag_background_table_addr as u16) << 12;

    let mut pattern_lo = s.ppu_peek(pattern_addr) as u16;
    let mut pattern_hi = s.ppu_peek(pattern_addr | 0x8) as u16;

    for i in 0..8 {
        let pixel_data = at_data | (pattern_lo & 0x1) | ((pattern_hi & 0x1) << 1);
        pattern_lo >>= 1;
        pattern_hi >>= 1;
        let mut ind = s.ppu.bg_data_index + 24 - 1 - i;
        if ind >= 24 {
            ind -= 24;
        }
        s.ppu.bg_data[ind] = pixel_data as u8;
    }
}

#[inline]
pub fn peek_register(s: &mut State, register: u16) -> u8 {
    catch_up(s);

    s.ppu.latch = match register {
        2 => {
            let data = (s.ppu.latch & 0x1F)
                | (s.ppu.sprite_overflow) << 5
                | (s.ppu.sprite0_hit as u8) << 6
                | (s.ppu.vblank) << 7;

            s.ppu.vblank = 0;
            s.ppu.w = 0;
            data
        }
        4 => {
            s.ppu.oam_1[s.ppu.oam_addr]
        }
        7 => {
            let mut data = s.ppu_peek(s.ppu.v);
            if s.ppu.v <= 0x3EFF {
                std::mem::swap(&mut data, &mut s.ppu.data_buffer);
            } else {
                s.ppu.data_buffer = s.ppu_peek(s.ppu.v - 0x1000);
            }

            s.ppu.v = s.ppu.v.wrapping_add(if s.ppu.flag_vram_increment == 0 {
                1
            } else {
                32
            });
            data
        }
        _ => s.ppu.latch,
    };
    s.ppu.latch
}

#[inline]
pub fn poke_register(s: &mut State, register: u16, data: u8) {
    catch_up(s);

    s.ppu.latch = data;
    match register {
        0 => {
            let old_nmi = s.ppu.flag_generate_nmi;
            
            s.ppu.t = (s.ppu.t & 0b1111_0011_1111_1111) | (((data & 0b11) as u16) << 10);

            s.ppu.flag_vram_increment = (data >> 2) & 0x1;
            s.ppu.flag_sprite_table_addr = (data >> 3) & 0x1;
            s.ppu.flag_background_table_addr = (data >> 4) & 0x1;
            s.ppu.flag_sprite_size = (data >> 5) & 0x1;
            s.ppu.flag_master_slave = (data >> 6) & 0x1;
            s.ppu.flag_generate_nmi = (data >> 7) & 0x1 > 0;

            if !old_nmi && s.ppu.flag_generate_nmi && s.ppu.vblank == 1 {
                s.cpu.pending_interrupt = cpu::InterruptKind::NMI;
            }
        }
        1 => {
            s.ppu.flag_grayscale = (data >> 0) & 0x1 > 0;
            s.ppu.flag_show_background_left = (data >> 1) & 0x1 > 0;
            s.ppu.flag_show_sprites_left = (data >> 2) & 0x1 > 0;
            s.ppu.flag_render_background = (data >> 3) & 0x1 > 0;
            s.ppu.flag_render_sprites = (data >> 4) & 0x1 > 0;
            s.ppu.flag_emphasize_red = (data >> 5) & 0x1 > 0;
            s.ppu.flag_emphasize_green = (data >> 6) & 0x1 > 0;
            s.ppu.flag_emphasize_blue = (data >> 7) & 0x1 > 0;
        }
        3 => {
            s.ppu.oam_addr = data as usize;
        }
        4 => {
            if !s.ppu.is_rendering {
                s.ppu.oam_1[s.ppu.oam_addr] = data;
                s.ppu.oam_addr = (s.ppu.oam_addr + 1) & 0xFF;
            }
        }
        5 => {
            if s.ppu.w == 0 {
                s.ppu.t = (s.ppu.t & 0b1111_1111_1110_0000) | ((data & 0b11111000) as u16 >> 3);
                s.ppu.x = (data & 0b111) as u16;
                s.ppu.w = 1;
            } else {
                s.ppu.t = (s.ppu.t & 0b1000_1100_0001_1111)
                    | ((data & 0b0000_0111) as u16) << 12
                    | ((data & 0b1111_1000) as u16) << 2;
                s.ppu.w = 0;
            }
        }
        6 => {
            if s.ppu.w == 0 {
                s.ppu.t = (s.ppu.t & 0b1000_0000_1111_1111) | ((data & 0b0011_1111) as u16) << 8;
                s.ppu.w = 1;
            } else {
                s.ppu.t = (s.ppu.t & 0xFF00) | (data as u16);
                s.ppu.v = s.ppu.t;
                s.ppu.w = 0;
            }
        }
        7 => {
            s.ppu_poke(s.ppu.v, data);
            s.ppu.v = s.ppu.v.wrapping_add(if s.ppu.flag_vram_increment == 0 {
                1
            } else {
                32
            });
        }
        0x4014 => {
            let cpu_cycles = s.cpu.cycles;
            let addr = (data as u16) << 8;
            for i in 0..256 {
                let data = s.cpu_peek(addr | (i as u16));
                s.ppu.oam_1[s.ppu.oam_addr] = data;
                s.ppu.oam_addr = (s.ppu.oam_addr + 1) & 0xFF;
            }
            s.cpu.cycles = cpu_cycles + 513 + (cpu_cycles & 0x1);
        }
        _ => {}
    };
}