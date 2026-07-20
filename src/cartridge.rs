#[derive(Clone, Default)]
pub struct RomHeader {
    prg_rom_size: u8, // in 16KB units
    chr_rom_size: u8, // in  8KB units
    flags6: u8,
    flags7: u8,
}

#[derive(Clone, Default)]
pub struct Cartridge {
    pub(crate) _header: RomHeader,
    pub(crate) prg_rom: Vec<u8>,
    pub(crate) chr_rom: Vec<u8>,
    pub(crate) mapper_id: u8,
    pub(crate) mirror_mode: u8,
    pub(crate) battery: bool,
}

impl Cartridge {
    pub fn load(data: &[u8]) -> Cartridge {
        assert!(data.len() >= 16);
        assert!(&data[0..4] == [0x4E, 0x45, 0x53, 0x1A], "not an iNES file");

        let header = RomHeader {
            prg_rom_size: data[4],
            chr_rom_size: data[5],
            flags6: data[6],
            flags7: data[7],
        };

        let mut index: usize = 16;
        let prg_rom = &data[index..(index + (16384 * header.prg_rom_size as usize))];
        let prg_rom = prg_rom.to_vec();
        index += 16384 * header.prg_rom_size as usize;
        let chr_rom = &data[index..(index + (8192 * header.chr_rom_size as usize))];
        let mut chr_rom = chr_rom.to_vec();
        // No need to advance `index` further; nothing reads the trailing bytes.

        if header.chr_rom_size == 0 {
            // 8KB of CHR RAM
            chr_rom = vec![0; 8192];
        }

        // --- The "DiskDude" Heuristic ---
        let mapper_lo = header.flags6 >> 4;
        let mut mapper_hi = header.flags7 & 0xF0;

        // If bytes 12-15 contain non-zero data, it's a dirty header. Ignore Byte 7's mapper bits.
        let is_dirty_header = data[12..16].iter().any(|&b| b != 0);
        if is_dirty_header {
            mapper_hi = 0;
        }

        Cartridge {
            prg_rom,
            chr_rom,
            mapper_id: mapper_hi | mapper_lo,
            mirror_mode: (header.flags6 & 0x1) | ((header.flags6 & 0x8) >> 2),
            battery: (header.flags6 & 0x02) != 0,
            _header: header,
        }
    }

    pub fn has_battery(&self) -> bool {
        self.battery
    }

    pub fn get_mapper_id(&self) -> u8 {
        self.mapper_id
    }

    pub fn get_mapper_name(&self) -> &'static str {
        match self.mapper_id {
            0 => "NROM",
            1 => "MMC1",
            2 => "UNROM",
            3 => "CNROM",
            4 => "MMC3",
            5 => "MMC5",
            7 => "AxROM",
            8 => "Nina-03",
            9 => "MMC2",
            11 => "Color Dreams",
            13 => "CPROM",
            15 => "CP-ROM",
            34 => "BNROM",
            41 => "Caltron",
            47 => "MMC3v2",
            64 => "RAMBO-1",
            65 => "MMC4",
            66 => "GxROM",
            68 => "Namco 175",
            69 => "Sunsoft FME-7",
            71 => "Camerica",
            79 => "Nina",
            105 => "NES-EvtROM",
            113 => "Sachen",
            118 => "TxSROM",
            119 => "TQROM",
            228 => "Action 52",
            232 => "Quattro",
            _ => "Unknown",
        }
    }
}