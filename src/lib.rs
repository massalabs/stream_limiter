//! This crate provides a `Limiter` struct that can be used to limit the rate at which a stream can be read or written.
//! This crate is based on the token bucket algorithm. When we want to read data and we are rate limited the packet aren't drop but we sleep.
//! Example:
//! ```
//! use stream_limiter::{Limiter, LimiterOptions};
//! use std::time::Duration;
//! use std::io::prelude::*;
//! use std::fs::File;
//!
//! let mut file = File::open("tests/resources/test.txt").unwrap();
//! let mut limiter = Limiter::new(file, Some(LimiterOptions::new(1, Duration::from_secs(1), 1)), None);
//! let mut buf = [0u8; 10];
//! let now = std::time::Instant::now();
//! limiter.read(&mut buf).unwrap();
//! assert_eq!(now.elapsed().as_secs(), 9);
//! ```
use std::{
    io::{self, Read, Write},
    time::Duration,
};

pub struct LimiterOptions {
    window_length: u128,
    window_time: Duration,
    bucket_size: usize,
}

impl LimiterOptions {
    pub fn new(window_length: u128, window_time: Duration, bucket_size: usize) -> LimiterOptions {
        LimiterOptions {
            window_length,
            window_time,
            bucket_size,
        }
    }
}

/// A `Limiter` is a wrapper around a stream that implement `Read` and `Write` that limits the rate at which it can be read or written.
/// The rate is given in byte/s.
pub struct Limiter<S>
where
    S: Read + Write,
{
    pub stream: S,
    read_opt: Option<LimiterOptions>,
    write_opt: Option<LimiterOptions>,
    last_read_check: Option<std::time::Instant>,
    last_write_check: Option<std::time::Instant>,
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
    pub fn new(
        stream: S,
        read_opt: Option<LimiterOptions>,
        write_opt: Option<LimiterOptions>,
    ) -> Limiter<S> {
        Limiter {
            stream,
            // We start at the beginning of last time window
            last_read_check: if let Some(LimiterOptions { window_time, .. }) = read_opt {
                Some(std::time::Instant::now().checked_sub(window_time).unwrap())
            } else {
                None
            },
            last_write_check: if let Some(LimiterOptions { window_time, .. }) = write_opt {
                Some(std::time::Instant::now().checked_sub(window_time).unwrap())
            } else {
                None
            },
            read_opt,
            write_opt,
        }
    }

    fn stream_cap_limit(&self) -> (Option<usize>, Option<usize>) {
        let read_cap = if let Some(LimiterOptions {
            window_length,
            bucket_size,
            ..
        }) = self.read_opt
        {
            Some(std::cmp::min(window_length as usize, bucket_size))
        } else {
            None
        };
        let write_cap = if let Some(LimiterOptions {
            window_length,
            bucket_size,
            ..
        }) = self.write_opt
        {
            Some(std::cmp::min(window_length as usize, bucket_size))
        } else {
            None
        };
        (read_cap, write_cap)
    }

    fn tokens_available(&self) -> (Option<usize>, Option<usize>) {
        let read_tokens = if let Some(LimiterOptions {
            window_length,
            window_time,
            bucket_size,
        }) = self.read_opt
        {
            Some(std::cmp::min(
                ((self.last_read_check.unwrap().elapsed().as_nanos() / window_time.as_nanos())
                    * window_length) as usize,
                bucket_size,
            ))
        } else {
            None
        };
        let write_tokens = if let Some(LimiterOptions {
            window_length,
            window_time,
            bucket_size,
        }) = self.write_opt
        {
            Some(std::cmp::min(
                ((self.last_write_check.unwrap().elapsed().as_nanos() / window_time.as_nanos())
                    * window_length) as usize,
                bucket_size,
            ))
        } else {
            None
        };
        (read_tokens, write_tokens)
    }

    pub fn limits(&self) -> (bool, bool) {
        (
            self.read_opt.is_some() && self.last_read_check.is_some(),
            self.write_opt.is_some() && self.last_write_check.is_some(),
        )
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
        let mut buf_left = buf.len();
        let readlimit = if let (Some(limit), _) = self.stream_cap_limit() {
            limit
        } else {
            return self.stream.read(buf);
        };
        let LimiterOptions { window_time, .. } = self.read_opt.as_ref().unwrap();
        while buf_left > 0 {
            let nb_bytes_readable = self.tokens_available().0.unwrap().min(buf_left);
            if nb_bytes_readable < readlimit.min(buf_left) {
                if let Some(lrc) = self.last_read_check {
                    std::thread::sleep((*window_time - lrc.elapsed()).max(Duration::from_nanos(0)));
                } else {
                    std::thread::sleep(*window_time);
                }
                continue;
            }
            // Before reading so that we don't count the time it takes to read
            self.last_read_check = Some(std::time::Instant::now());
            let buf_read_end = read + nb_bytes_readable.min(buf_left);
            let read_now = self.stream.read(&mut buf[read..buf_read_end])?;
            if read_now < nb_bytes_readable {
                break;
            }
            read += read_now;
            buf_left -= read_now;
        }
        self.last_read_check = Some(std::time::Instant::now());
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
        let mut buf_left = buf.len();
        let writelimit = if let (_, Some(limit)) = self.stream_cap_limit() {
            limit
        } else {
            return self.stream.write(buf);
        };
        let LimiterOptions { window_time, .. } = self.write_opt.as_ref().unwrap();
        while buf_left > 0 {
            let nb_bytes_writable = self.tokens_available().1.unwrap().min(buf_left);
            if nb_bytes_writable < writelimit.min(buf_left) {
                if let Some(lwc) = self.last_write_check {
                    std::thread::sleep((*window_time - lwc.elapsed()).max(Duration::from_nanos(0)));
                } else {
                    std::thread::sleep(*window_time);
                }
                continue;
            }
            // Before reading so that we don't count the time it takes to read
            self.last_write_check = Some(std::time::Instant::now());
            let buf_write_end = write + nb_bytes_writable.min(buf_left);
            let write_now = self.stream.write(&buf[write..buf_write_end])?;
            if write_now < nb_bytes_writable {
                break;
            }
            write += write_now;
            buf_left -= write_now;
        }
        self.last_write_check = Some(std::time::Instant::now());
        Ok(write)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
