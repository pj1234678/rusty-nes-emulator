use std::io::{self, Read, Write};

pub trait WriteState {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()>;
}

pub trait ReadState: Sized {
    fn read(reader: &mut dyn Read) -> io::Result<Self>;
}

impl WriteState for u8 {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        writer.write_all(&[*self])
    }
}

impl ReadState for u8 {
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        Ok(buf[0])
    }
}

impl WriteState for u16 {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        writer.write_all(&self.to_le_bytes())
    }
}

impl ReadState for u16 {
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        let mut buf = [0u8; 2];
        reader.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }
}

impl WriteState for u64 {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        writer.write_all(&self.to_le_bytes())
    }
}

impl ReadState for u64 {
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }
}

impl WriteState for bool {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        writer.write_all(&[*self as u8])
    }
}

impl ReadState for bool {
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        Ok(buf[0] != 0)
    }
}

impl WriteState for f32 {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        writer.write_all(&self.to_le_bytes())
    }
}

impl ReadState for f32 {
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(f32::from_le_bytes(buf))
    }
}

impl WriteState for usize {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        (*self as u64).write(writer)
    }
}

impl ReadState for usize {
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        Ok(u64::read(reader)? as usize)
    }
}

impl WriteState for Vec<u8> {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        (self.len() as u64).write(writer)?;
        writer.write_all(self)
    }
}

impl ReadState for Vec<u8> {
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        let len = u64::read(reader)? as usize;
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }
}

impl WriteState for Vec<f32> {
    fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
        (self.len() as u64).write(writer)?;
        for item in self {
            item.write(writer)?;
        }
        Ok(())
    }
}

impl ReadState for Vec<f32> {
    fn read(reader: &mut dyn Read) -> io::Result<Self> {
        let len = u64::read(reader)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(f32::read(reader)?);
        }
        Ok(vec)
    }
}

macro_rules! impl_write_state_bytes_array {
    ($($n:expr),+) => {
        $(
            impl WriteState for [u8; $n] {
                fn write(&self, writer: &mut dyn Write) -> io::Result<()> {
                    writer.write_all(self)
                }
            }

            impl ReadState for [u8; $n] {
                fn read(reader: &mut dyn Read) -> io::Result<Self> {
                    let mut buf = [0u8; $n];
                    reader.read_exact(&mut buf)?;
                    Ok(buf)
                }
            }
        )+
    };
}

impl_write_state_bytes_array!(8, 24, 32, 256, 2048, 8192);

#[macro_export]
macro_rules! impl_write_state_generic_array {
    ($t:ty, $n:expr) => {
        impl $crate::save_state::WriteState for [$t; $n] {
            fn write(&self, writer: &mut dyn std::io::Write) -> std::io::Result<()> {
                for item in self.iter() {
                    item.write(writer)?;
                }
                Ok(())
            }
        }

        impl $crate::save_state::ReadState for [$t; $n] {
            fn read(reader: &mut dyn std::io::Read) -> std::io::Result<Self> {
                use std::convert::TryInto;
                let mut items = Vec::with_capacity($n);
                for _ in 0..$n {
                    items.push(<$t>::read(reader)?);
                }
                items.try_into().map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "array length mismatch"))
            }
        }
    };
}
