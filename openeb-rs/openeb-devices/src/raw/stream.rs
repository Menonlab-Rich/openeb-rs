use crate::types::RawEventBuffer;
use openeb_core::hal::errors::StreamError;
use openeb_core::hal::facilities::{EventsStreamFacility, FacilityError, FacilityResult};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub(crate) struct RREventStream<const N: usize> {
    file: File,
    buffer: RawEventBuffer<N>,
    eof: bool,
    started: bool,
}

impl<const N: usize> RREventStream<N> {
    pub(crate) fn new(file: File) -> Self {
        Self {
            file,
            buffer: RawEventBuffer::<N>::new(),
            eof: false,
            started: false,
        }
    }

    pub(crate) fn seek_to_offset(&mut self, byte_offset: u64) -> Result<(), std::io::Error> {
        self.file.seek(SeekFrom::Start(byte_offset))?;
        self.eof = false;
        Ok(())
    }
}

impl<const N: usize> EventsStreamFacility for RREventStream<N> {
    fn start(&mut self) -> FacilityResult<()> {
        self.started = true;
        Ok(())
    }

    fn stop(&mut self) -> FacilityResult<()> {
        self.started = false;
        Ok(())
    }

    fn poll_buffer(&mut self) -> FacilityResult<(&[u8], usize)> {
        if !self.started {
            return Err(FacilityError::Stream(StreamError::Disconnected));
        }
        self.wait_next_buffer()
    }

    fn wait_next_buffer(&mut self) -> FacilityResult<(&[u8], usize)> {
        if self.eof {
            return Err(FacilityError::Stream(StreamError::EndOfFile));
        }

        match self.file.read(&mut self.buffer) {
            Ok(0) => {
                self.eof = true;
                Err(FacilityError::Stream(StreamError::EndOfFile))
            }
            Ok(bytes_read) => Ok((&self.buffer[..bytes_read], bytes_read)),
            Err(err) => {
                self.eof = true;
                Err(FacilityError::Stream(StreamError::IoError(err)))
            }
        }
    }
}
