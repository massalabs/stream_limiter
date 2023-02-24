//! This crate provides a `Limiter` struct that can be used to limit the rate at which a stream can be read or written.
//! This crate is based on the token bucket algorithm. When we want to read data and we are rate limited the packet aren't drop but we sleep.
//! Example:
//! ```
//! use stream_limiter::Limiter;
//! use std::time::Duration;
//! use std::io::prelude::*;
//! use std::fs::File;
//!
//! let mut file = File::open("tests/resources/test.txt").unwrap();
//! let mut limiter = Limiter::new(file, 1, Duration::from_secs(1));
//! let mut buf = [0u8; 10];
//! let now = std::time::Instant::now();
//! limiter.read(&mut buf).unwrap();
//! assert_eq!(now.elapsed().as_secs(), 9);
//! ```
use std::{io::{self, Read, Write}, time::Duration};

/// A `Limiter` is a wrapper around a stream that implement `Read` and `Write` that limits the rate at which it can be read or written.
/// The rate is given in byte/s.
pub struct Limiter<S>
where
    S: Read + Write,
{
    window_length: u128,
    window_time: Duration,
    stream: S,
    last_read_check: std::time::Instant,
    last_write_check: std::time::Instant,
}

impl<S> Limiter<S>
where
    S: Read + Write,
{
    /// Create a new `Limiter` with the given `stream` and rate limiting:
    /// - `window_length`: The number of bytes that can be read or written in a given time window.
    /// - `window_time`: The time window in which `window_length` bytes can be read or written.
    /// 
    /// We initialize the limiter as if one period has already passed so that the first read/write is instant.
    pub fn new(stream: S, window_length: u128, window_time: Duration) -> Limiter<S> {
        Limiter {
            window_length,
            window_time,
            stream,
            // We start at the beginning of last time window
            last_read_check: std::time::Instant::now() - window_time,
            last_write_check: std::time::Instant::now() - window_time,
        }
    }
}

impl<S> Read for Limiter<S>
where
    S: Read + Write,
{
    /// Read a stream at a given rate. If the rate is 1 byte/s, it will take 1 second to read 1 byte. (except the first time which is instant)
    /// If you didn't read for 10 secondes in this stream and you try to read 10 bytes, it will read instantly.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read = 0;
        let buf_len = buf.len();
        while read < buf_len {
            let nb_bytes_readable = std::cmp::min(
                ((self.last_read_check.elapsed().as_nanos() / self.window_time.as_nanos()) * self.window_length) as usize,
                buf_len,
            );
            if nb_bytes_readable < self.window_length as usize {
                std::thread::sleep(self.window_time);
                continue;
            }
            // Before reading so that we don't count the time it takes to read
            self.last_read_check = std::time::Instant::now();
            let read_now = self.stream.read(&mut buf[..nb_bytes_readable])?;
            if read_now < nb_bytes_readable {
                break;
            }
            read += read_now;
        }
        self.last_read_check = std::time::Instant::now();
        Ok(read)
    }
}

impl<S> Write for Limiter<S>
where
    S: Read + Write,
{
    /// Write a stream at a given rate. If the rate is 1 byte/s, it will take 1 second to write 1 byte. (except the first time which is instant)
    /// If you didn't write for 10 secondes in this stream and you try to write 10 bytes, it will write instantly.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut write = 0;
        let buf_len = buf.len();
        while write < buf_len {
            let nb_bytes_writable = std::cmp::min(
                ((self.last_write_check.elapsed().as_nanos() / self.window_time.as_nanos()) * self.window_length) as usize,
                buf_len,
            );
            if nb_bytes_writable < self.window_length as usize {
                std::thread::sleep(self.window_time);
                continue;
            }
            // Before reading so that we don't count the time it takes to read
            self.last_write_check = std::time::Instant::now();
            let write_now = self.stream.write(&buf[..nb_bytes_writable])?;
            if write_now < nb_bytes_writable {
                break;
            }
            write += write_now;
        }
        self.last_write_check = std::time::Instant::now();
        Ok(write)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
