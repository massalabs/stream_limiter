//! This crate provides a `Limiter` struct that can be used to limit the rate at which a stream can be read or written.
//! This crate is based on the token bucket algorithm. When we want to read data and we are rate limited the packet aren't drop but we sleep.
//! Example:
//! ```
//! use stream_limiter::{Limiter, LimiterOptions};
//! use std::time::Duration;
//! use std::io::prelude::*;
//! use std::fs::File;
//!
//! let mut file = File::open("test_resources/test.txt").unwrap();
//! let mut limiter = Limiter::new(file, Some(LimiterOptions::new(1, Duration::from_secs(1), 1)), None);
//! let mut buf = [0u8; 10];
//! let now = std::time::Instant::now();
//! limiter.read(&mut buf).unwrap();
//! //assert_eq!(now.elapsed().as_secs(), 9);
//! ```
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};
use std::{debug_assert, debug_assert_ne};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct LimiterOptions {
    pub window_length: u64,
    pub window_time: Duration,
    pub bucket_size: u64,
    pub min_operation_size: u64,
}

impl LimiterOptions {
    pub fn new(window_length: u64, window_time: Duration, bucket_size: u64) -> LimiterOptions {
        LimiterOptions {
            window_length,
            window_time,
            bucket_size,
            min_operation_size: 1,
        }
    }
}

impl LimiterOptions {
    pub fn set_min_operation_size(&mut self, val: u64) {
        assert_ne!(val, 0);
        assert!(
            self.bucket_size >= val,
            "Bucket size: {}, min_operation_size: {}",
            self.bucket_size,
            val
        );
        self.min_operation_size = val;
    }

    pub fn intersect(&self, other: &LimiterOptions) -> LimiterOptions {
        let rate_a = self.window_length.min(self.bucket_size) as f64;
        let tw_a = self.window_time.as_nanos() as f64;
        let speed_a = (rate_a / tw_a) * 1_000_000.0;

        let rate_b = other.window_length.min(other.bucket_size) as f64;
        let tw_b = other.window_time.as_nanos() as f64;
        let speed_b = (rate_b / tw_b) * 1_000_000.0;

        if speed_a < speed_b {
            self.clone()
        } else {
            other.clone()
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
    additionnal_tokens: (u64, u64),

    #[cfg(test)]
    pub blocking_duration: (Duration, Duration),
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
        // ident: &'static str,
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
            additionnal_tokens: (0, 0),
            read_opt,
            write_opt,

            #[cfg(test)]
            blocking_duration: (Duration::ZERO, Duration::ZERO),
        }
    }

    pub fn get_stream(self) -> S {
        self.stream
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

    fn tokens_available(&self) -> (Option<u64>, Option<u64>) {
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
            let window_time =
                u64::try_from(window_time.as_nanos()).expect("Window time nanos > u64::MAX");
            Some(
                std::cmp::min(lrc.saturating_mul(window_length) / window_time, bucket_size)
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
            let window_time =
                u64::try_from(window_time.as_nanos()).expect("Window time nanos > u64::MAX");
            Some(
                std::cmp::min(lwc.saturating_mul(window_length) / window_time, bucket_size)
                    .saturating_add(self.additionnal_tokens.1),
            )
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

    pub fn compute_tsleep_per_byte(&self, opt: &LimiterOptions) -> Duration {
        opt.window_time / TryInto::<u32>::try_into(opt.window_length).unwrap()
    }
}

impl<S> Read for Limiter<S>
where
    S: Read + Write,
{
    /// Read a stream at a given rate. If the rate is 1 byte/s, it will take 1 second to read 1 byte. (except the first time which is instant)
    /// If you didn't read for 10 secondes in this stream and you try to read 10 bytes, it will read instantly.
    #[allow(unused_variables)]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read: u64 = 0;
        let mut buf_left = u64::try_from(buf.len()).expect("R buflen to u64");
        let readlimit = if let (Some(limit), _) = self.stream_cap_limit() {
            limit
        } else {
            #[cfg(test)]
            {
                let read_start_instant = Instant::now();
                let nb = self.stream.read(buf)?;
                self.blocking_duration.0 += read_start_instant.elapsed();
                return Ok(nb);
            }
            #[cfg(not(test))]
            {
                return self.stream.read(buf);
            }
        };
        let tsleep = self.compute_tsleep_per_byte(self.read_opt.as_ref().unwrap());
        let opts = self.read_opt.as_ref().unwrap();
        let sleep_threshold = readlimit.max(opts.min_operation_size);
        debug_assert_ne!(tsleep.as_nanos(), 0);
        while buf_left > 0 {
            let nb_bytes_readable = self.tokens_available().0.unwrap().min(buf_left);
            if nb_bytes_readable < sleep_threshold.min(buf_left).min(opts.bucket_size) {
                let nb_left: u32 = sleep_threshold
                    .min(buf_left)
                    .min(opts.bucket_size)
                    .saturating_sub(nb_bytes_readable)
                    .try_into()
                    .expect("Read nb left > u32::MAX");
                std::thread::sleep(tsleep * nb_left);

                #[cfg(debug_assertions)]
                {
                    if nb_bytes_readable != opts.bucket_size {
                        let new_nb_bytes_readable = self.tokens_available().0.unwrap();
                        debug_assert!(new_nb_bytes_readable > nb_bytes_readable,
                            "\n{:?}\nTsleep: {tsleep:?} x {nb_left} = {:?}\nReadlimit: {readlimit}\n{nb_bytes_readable} == {new_nb_bytes_readable}",
                            self.read_opt.as_ref(),
                            tsleep * nb_left,
                        );
                    }
                }
                continue;
            }
            // Before reading so that we don't count the time it takes to read
            self.last_read_check = Some(std::time::Instant::now());
            let read_start = usize::try_from(read).expect("R read_start to usize");
            let read_end = usize::try_from(read.saturating_add(nb_bytes_readable.min(buf_left)))
                .expect("R read_end to usize");
            let read_start_instant = Instant::now();
            let read_now = u64::try_from(self.stream.read(&mut buf[read_start..read_end])?)
                .expect("R read_now to u64");
            #[cfg(test)]
            {
                self.blocking_duration.0 += read_start_instant.elapsed();
            }
            self.additionnal_tokens = (
                nb_bytes_readable.saturating_sub(read_now),
                self.additionnal_tokens.1,
            );
            read = read.saturating_add(read_now);
            buf_left = buf_left.saturating_sub(read_now);
            if read_now == 0 {
                break;
            }
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
    #[allow(unused_variables)]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut write: u64 = 0;
        let mut buf_left = u64::try_from(buf.len()).expect("W buflen to u64");
        let writelimit = if let (_, Some(limit)) = self.stream_cap_limit() {
            limit
        } else {
            #[cfg(test)]
            {
                let write_start_instant = Instant::now();
                let nb = self.stream.write(buf)?;
                self.blocking_duration.1 += write_start_instant.elapsed();
                return Ok(nb);
            }
            #[cfg(not(test))]
            {
                return self.stream.write(buf);
            }
        };
        let tsleep = self.compute_tsleep_per_byte(self.write_opt.as_ref().unwrap());
        debug_assert_ne!(tsleep.as_nanos(), 0);
        let opts = self.write_opt.as_ref().unwrap();
        let min_operation_size = opts.min_operation_size;

        let sleep_threshold = writelimit.max(min_operation_size);
        while buf_left > 0 {
            let nb_bytes_writable = self.tokens_available().1.unwrap().min(buf_left);
            if nb_bytes_writable < sleep_threshold.min(buf_left).min(opts.bucket_size) {
                let nb_left: u32 = sleep_threshold
                    .min(buf_left)
                    .saturating_sub(nb_bytes_writable)
                    .try_into()
                    .expect("Write nb left > u32::MAX");
                std::thread::sleep(tsleep * nb_left);

                #[cfg(debug_assertions)]
                {
                    if nb_bytes_writable != opts.bucket_size {
                        let new_nb_bytes_writable = self.tokens_available().1.unwrap();
                        debug_assert!(
                            new_nb_bytes_writable > nb_bytes_writable,
                            "\n{:?}\nTsleep: {tsleep:?}\nWritelimit: {writelimit}\n",
                            self.write_opt.as_ref(),
                        );
                    }
                }
                continue;
            }
            // Before reading so that we don't count the time it takes to read
            self.last_write_check = Some(std::time::Instant::now());
            let write_start = usize::try_from(write).expect("W write_start to usize");
            let write_end = usize::try_from(write.saturating_add(nb_bytes_writable.min(buf_left)))
                .expect("W write_end to usize");
            let write_start_instant = Instant::now();
            let write_now = u64::try_from(self.stream.write(&buf[write_start..write_end])?)
                .expect("W write_now_ to u64");
            #[cfg(test)]
            {
                self.blocking_duration.1 += write_start_instant.elapsed();
            }
            self.additionnal_tokens = (
                self.additionnal_tokens.0,
                nb_bytes_writable.saturating_sub(write_now),
            );
            write = write.saturating_add(write_now);
            buf_left = buf_left.saturating_sub(write_now);
            if write_now == 0 {
                break;
            }
        }
        self.last_write_check = Some(std::time::Instant::now());
        Ok(usize::try_from(write).expect("W return to usize"))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
