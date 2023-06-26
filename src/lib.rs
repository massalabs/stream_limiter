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
    debug_assert,
    io::{self, Read, Write},
    time::Duration, debug_assert_ne,
};

#[derive(Clone, Debug)]
pub struct LimiterOptions {
    pub window_length: u64,
    pub window_time: Duration,
    pub bucket_size: u64,
}

impl LimiterOptions {
    pub fn new(
        window_length: u64,
        window_time: Duration,
        bucket_size: u64,
    ) -> LimiterOptions {
        LimiterOptions {
            window_length,
            window_time,
            bucket_size,
        }
    }
}

impl LimiterOptions {
    pub fn intersect(&self, _other: &LimiterOptions) -> LimiterOptions {
        // TODO    Implement this intersection
        todo!()        
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
    additionnal_tokens: (u64, u64),
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
            last_read_check: if read_opt.is_some() {
                Some(std::time::Instant::now())
            } else {
                None
            },
            last_write_check: if write_opt.is_some() {
                Some(std::time::Instant::now())
            } else {
                None
            },
            additionnal_tokens: (
                if let Some(ref ro) = read_opt { ro.window_length } else { 0 },
                if let Some(ref wo) = write_opt { wo.window_length } else { 0 },
            ),
            read_opt,
            write_opt,
        }
    }

    fn stream_cap_limit(&self) -> (Option<u64>, Option<u64>) {
        let read_cap = if let Some(LimiterOptions {
            window_length,
            bucket_size,
            ..
        }) = self.read_opt
        {
            Some(std::cmp::min(window_length, bucket_size))
        } else {
            None
        };
        let write_cap = if let Some(LimiterOptions {
            window_length,
            bucket_size,
            ..
        }) = self.write_opt
        {
            Some(std::cmp::min(window_length, bucket_size))
        } else {
            None
        };
        (read_cap, write_cap)
    }

    fn tokens_available(&mut self) -> (Option<u64>, Option<u64>) {
        let read_tokens = if let Some(LimiterOptions {
            window_length,
            window_time,
            bucket_size,
            ..
        }) = self.read_opt
        {
            let lrc = match u64::try_from(self.last_read_check.unwrap().elapsed().as_nanos()) {
                Ok(n) => n,
                // Will cap the last_read_check at a duration of about 584 years
                Err(_) => u64::MAX,
            };
            let window_time = u64::try_from(window_time.as_nanos())
                .expect("Window time nanos > u64::MAX");
            Some(
                std::cmp::min(
                    lrc.saturating_mul(window_length) / window_time,
                    bucket_size,
                )
                .saturating_add(self.additionnal_tokens.0),
            )
        } else {
            None
        };
        let write_tokens = if let Some(LimiterOptions {
            window_length,
            window_time,
            bucket_size,
            ..
        }) = self.write_opt
        {
            let lwc = match u64::try_from(self.last_write_check.unwrap().elapsed().as_nanos()) {
                Ok(n) => n,
                // Will cap the last_read_check at a duration of about 584 years
                Err(_) => u64::MAX,
            };
            let window_time = u64::try_from(window_time.as_nanos())
                .expect("Window time nanos > u64::MAX");
            Some(
                std::cmp::min(
                    lwc.saturating_mul(window_length) / window_time,
                    bucket_size,
                )
                .saturating_add(self.additionnal_tokens.1),
            )
        } else {
            None
        };
        self.additionnal_tokens = (0, 0);
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
        let mut read: u64 = 0;
        let mut buf_left = u64::try_from(buf.len()).expect("R buflen to u64");
        let readlimit = if let (Some(limit), _) = self.stream_cap_limit() {
            limit
        } else {
            return self.stream.read(buf);
        };
        let LimiterOptions { window_time, .. } = self.read_opt.as_ref().unwrap();
        let tsleep = *window_time / (TryInto::<u32>::try_into(readlimit).unwrap());
        debug_assert_ne!(tsleep.as_nanos(), 0);
        while buf_left > 0 {
            let nb_bytes_readable = self.tokens_available().0.unwrap().min(buf_left);
            if nb_bytes_readable < readlimit.min(buf_left) {
                let elapsed = if let Some(lrc) = self.last_read_check {
                    lrc.elapsed()
                } else {
                    Duration::ZERO
                };
                std::thread::sleep(tsleep.saturating_sub(elapsed));
                debug_assert!(self.tokens_available().0.unwrap() > 0);
                continue;
            }
            // Before reading so that we don't count the time it takes to read
            self.last_read_check = Some(std::time::Instant::now());
            let read_start = usize::try_from(read).expect("R read_start to usize");
            let read_end = usize::try_from(read.saturating_add(nb_bytes_readable.min(buf_left)))
                .expect("R read_end to usize");
            let read_now = u64::try_from(self.stream.read(&mut buf[read_start..read_end])?)
                .expect("R read_now to u64");
            if read_now == 0 {
                break;
            }
            if read_now < nb_bytes_readable {
                self.additionnal_tokens.0 = self
                    .additionnal_tokens
                    .0
                    .saturating_add(nb_bytes_readable.saturating_sub(read_now));
            }
            read = read.saturating_add(read_now);
            buf_left = buf_left.saturating_sub(read_now);
        }
        self.last_read_check = Some(std::time::Instant::now());
        Ok(usize::try_from(read).expect("R return to usize"))
    }
}

impl<S> Write for Limiter<S>
where
    S: Read + Write,
{
    /// Write a stream at a given rate. If the rate is 1 byte/s, it will take 1 second to write 1 byte. (except the first time which is instant)
    /// If you didn't write for 10 secondes in this stream and you try to write 10 bytes, it will write instantly.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut write: u64 = 0;
        let mut buf_left = u64::try_from(buf.len()).expect("W buflen to u64");
        let writelimit = if let (_, Some(limit)) = self.stream_cap_limit() {
            limit
        } else {
            return self.stream.write(buf);
        };
        let LimiterOptions { window_time, .. } = self.write_opt.as_ref().unwrap();
        let tsleep = *window_time / (TryInto::<u32>::try_into(writelimit).unwrap());
        debug_assert_ne!(tsleep.as_nanos(), 0);
        while buf_left > 0 {
            let nb_bytes_writable = self.tokens_available().1.unwrap().min(buf_left);
            if nb_bytes_writable < writelimit.min(buf_left) {
                let elapsed = if let Some(lwc) = self.last_write_check {
                    lwc.elapsed()
                } else {
                    Duration::ZERO
                };
                std::thread::sleep(tsleep.saturating_sub(elapsed));
                debug_assert!(self.tokens_available().1.unwrap() > 0);
                continue;
            }
            // Before reading so that we don't count the time it takes to read
            self.last_write_check = Some(std::time::Instant::now());
            let write_start = usize::try_from(write).expect("W write_start to usize");
            let write_end = usize::try_from(write.saturating_add(nb_bytes_writable.min(buf_left)))
                .expect("W write_end to usize");
            let write_now = u64::try_from(self.stream.write(&buf[write_start..write_end])?)
                .expect("W write_now_ to u64");
            if write_now < nb_bytes_writable {
                break;
            }
            if write_now < nb_bytes_writable {
                self.additionnal_tokens.1 = self
                    .additionnal_tokens
                    .1
                    .saturating_add(nb_bytes_writable.saturating_sub(write_now));
            }
            write = write.saturating_add(write_now);
            buf_left = buf_left.saturating_sub(write_now);
        }
        self.last_write_check = Some(std::time::Instant::now());
        Ok(usize::try_from(write).expect("W return to usize"))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
