use std::io::Read;

pub struct ReadUntil<'a, R: ?Sized> {
    reader: &'a mut R,
    buf: Vec<u8>,
    read: usize,
    until: Vec<u8>,
}

pub fn get_reader<'a, R: Read + ?Sized>(reader: &'a mut R, until: &[u8]) -> ReadUntil<'a, R> {
    ReadUntil {
        reader,
        buf: Vec::new(),
        read: 0,
        until: until.to_vec(),
    }
}

impl<R: Read + ?Sized> Read for ReadUntil<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut read = 0;
        while read < buf.len() {
            if self.read == self.buf.len() {
                self.buf.clear();
                let mut tmp = [0; 1024];
                let n = self.reader.read(&mut tmp)?;
                if n == 0 {
                    break;
                }
                self.buf.extend_from_slice(&tmp[..n]);
                self.read = 0;
            }
            let n = std::cmp::min(buf.len() - read, self.buf.len() - self.read);
            let end = self.read + n;
            let found = self.buf[self.read..end]
                .windows(self.until.len())
                .position(|w| w == self.until);
            if let Some(pos) = found {
                let end = self.read + pos + self.until.len();
                buf[read..read + pos].copy_from_slice(&self.buf[self.read..end - self.until.len()]);
                self.read = end;
                return Ok(read + pos);
            } else {
                buf[read..read + n].copy_from_slice(&self.buf[self.read..end]);
                self.read = end;
                read += n;
            }
        }
        self.buf.clear();
        Ok(read)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        let mut read = 0;
        loop {
            if self.read == self.buf.len() {
                self.buf.clear();
                let mut tmp = [0; 1024];
                let n = self.reader.read(&mut tmp)?;
                if n == 0 {
                    break;
                }
                self.buf.extend_from_slice(&tmp[..n]);
                self.read = 0;
            }
            if self.read > self.buf.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Read position exceeds buffer length",
                ));
            }
            let n = self.buf.len() - self.read;
            let end = self.read + n;
            let found = self.buf[self.read..end]
                .windows(self.until.len())
                .position(|w| w == self.until);
            if let Some(pos) = found {
                let end = self.read + pos + self.until.len();
                buf.extend_from_slice(&self.buf[self.read..end - self.until.len()]);
                self.read = end;
                return Ok(read + pos);
            } else {
                buf.extend_from_slice(&self.buf[self.read..end]);
                self.read = end;
                read += n;
            }
        }
        self.buf.clear();
        Ok(read)
    }
}
