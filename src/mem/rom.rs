const NES_TAG: [u8; 4] = [0x4e, 0x45, 0x53, 0x1a];
const PRG_PAGE_SIZE: usize = 16384;
const CHR_PAGE_SIZE: usize = 8192;

#[derive(Debug, PartialEq)]
pub enum Mirroring {
    Vertical,
    Horizontal,
    FourScreen,
}

pub struct Rom {
    pub mirroring: Mirroring,
    pub mapper: u8, // If 0, CHR and PRG ROMs are accessible directly and independantly (Mapper 0)
    pub prg_rom: Vec<u8>,
    pub chr_rom: Vec<u8>,
}

impl Rom {
    pub fn new(raw: &Vec<u8>) -> Result<Rom, String> {
        if &raw[0..4] != NES_TAG {
            return Err("File does not contain the iNES file descriptor".to_string());
        }
        let mapper = (raw[7] & 0b1111_0000) | (raw[6] >> 4);

        let mut ines_ver = (raw[7] >> 2) & 0b11;
        ines_ver = if ines_ver == 0b10 {
            2
        } else if ines_ver == 0b00 {
            1
        } else {
            return Err("Invalid iNES version".to_string());
        };

        if ines_ver == 2 {
            return Err("iNES 2.0 is not supported".to_string());
        }

        let four_screen = raw[7] & 0b1000 != 0;
        let vertical_mirroring = raw[6] & 0b1 != 0;
        let mirroring = match (four_screen, vertical_mirroring) {
            (true, _) => Mirroring::FourScreen,
            (false, false) => Mirroring::Horizontal,
            (false, true) => Mirroring::Vertical,
        };

        let prg_size = raw[4] as usize * PRG_PAGE_SIZE;
        let chr_size = raw[5] as usize * CHR_PAGE_SIZE;

        let trainer = raw[6] & 0b100 != 0;

        let prg_start = 16 + if trainer { 512 } else { 0 };
        let chr_start = prg_start + prg_size;

        Ok(Rom {
            mirroring,
            mapper,
            prg_rom: raw[prg_start..(prg_start + prg_size)].to_vec(),
            chr_rom: raw[chr_start..(chr_start + chr_size)].to_vec(),
        })
    }
}

pub mod test {

    use super::*;

    struct TestRom {
        header: Vec<u8>,
        trainer: Option<Vec<u8>>,
        pgp_rom: Vec<u8>,
        chr_rom: Vec<u8>,
    }

    fn create_rom(rom: TestRom) -> Vec<u8> {
        let mut result = Vec::with_capacity(
            rom.header.len()
                + rom.trainer.as_ref().map_or(0, |t| t.len())
                + rom.pgp_rom.len()
                + rom.chr_rom.len(),
        );

        result.extend(&rom.header);
        if let Some(t) = rom.trainer {
            result.extend(t);
        }
        result.extend(&rom.pgp_rom);
        result.extend(&rom.chr_rom);

        result
    }

    pub fn test_rom(program: Vec<u8>) -> Rom {
        let mut pgp_rom_contents = program;
        pgp_rom_contents.resize(2 * PRG_PAGE_SIZE, 0);

        let test_rom = create_rom(TestRom {
            header: vec![
                0x4E, 0x45, 0x53, 0x1A, 0x02, 0x01, 0x31, 00, 00, 00, 00, 00, 00, 00, 00, 00,
            ],
            trainer: None,
            pgp_rom: pgp_rom_contents,
            chr_rom: vec![2; 1 * CHR_PAGE_SIZE],
        });

        Rom::new(&test_rom).unwrap()
    }

    #[test]
    fn test() {
        let test_rom = create_rom(TestRom {
            header: vec![
                0x4E, 0x45, 0x53, 0x1A, 0x02, 0x01, 0x31, 00, 00, 00, 00, 00, 00, 00, 00, 00,
            ],
            trainer: None,
            pgp_rom: vec![1; 2 * PRG_PAGE_SIZE],
            chr_rom: vec![2; 1 * CHR_PAGE_SIZE],
        });

        let rom: Rom = Rom::new(&test_rom).unwrap();

        assert_eq!(rom.chr_rom, vec!(2; 1 * CHR_PAGE_SIZE));
        assert_eq!(rom.prg_rom, vec!(1; 2 * PRG_PAGE_SIZE));
        assert_eq!(rom.mapper, 3);
        assert_eq!(rom.mirroring, Mirroring::Vertical);
    }

    #[test]
    fn test_with_trainer() {
        let test_rom = create_rom(TestRom {
            header: vec![
                0x4E,
                0x45,
                0x53,
                0x1A,
                0x02,
                0x01,
                0x31 | 0b100,
                00,
                00,
                00,
                00,
                00,
                00,
                00,
                00,
                00,
            ],
            trainer: Some(vec![0; 512]),
            pgp_rom: vec![1; 2 * PRG_PAGE_SIZE],
            chr_rom: vec![2; 1 * CHR_PAGE_SIZE],
        });

        let rom: Rom = Rom::new(&test_rom).unwrap();

        assert_eq!(rom.chr_rom, vec!(2; 1 * CHR_PAGE_SIZE));
        assert_eq!(rom.prg_rom, vec!(1; 2 * PRG_PAGE_SIZE));
        assert_eq!(rom.mapper, 3);
        assert_eq!(rom.mirroring, Mirroring::Vertical);
    }

    #[test]
    fn test_nes2_is_not_supported() {
        let test_rom = create_rom(TestRom {
            header: vec![
                0x4E, 0x45, 0x53, 0x1A, 0x01, 0x01, 0x31, 0x8, 00, 00, 00, 00, 00, 00, 00, 00,
            ],
            trainer: None,
            pgp_rom: vec![1; 1 * PRG_PAGE_SIZE],
            chr_rom: vec![2; 1 * CHR_PAGE_SIZE],
        });
        let rom = Rom::new(&test_rom);
        match rom {
            Result::Ok(_) => assert!(false, "should not load rom"),
            Result::Err(str) => assert_eq!(str, "iNES 2.0 is not supported"),
        }
    }
}
