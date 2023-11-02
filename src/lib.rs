//! This crate provides a `Limiter` struct that can be used to limit the rate
//! at which a stream can be read or written.
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
//! assert_eq!(now.elapsed().as_secs(), 10);
//! ```
use std::debug_assert;
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct LimiterOptions {
    pub window_length: u64, // How many bytes to be read on the window_time period
    pub window_time: Duration, // Time spent to read window_length bytes
    pub bucket_size: u64,   // Maximum number of bytes to be prepared for future read
    pub timeout: Option<Duration>, // Raise an error if this timeout is passed

    // Store constants based on options to avoid re-computation at runtime
    pub tsleep: Duration,      // Time to sleep for 1 byte of data
    pub wtime_ns: u64,         // Window time as nanoseconds
    pub stream_cap_limit: u64, // Limit between the window_length and bucket_size
    pub sleep_threshold: u64,  // Value under which we have to sleep to get more tokens
}

impl LimiterOptions {
    pub fn new(
        mut window_length: u64,
        mut window_time: Duration,
        bucket_size: u64,
    ) -> LimiterOptions {
        let mut wlen = window_length;
        let mut wtime = window_time;
        // Divide the window_length and window_time as long as we overflow u32::MAX
        // As it's not possible to divite Duration by u64.
        while wlen > u32::MAX as u64 {
            wlen /= 2;
            wtime /= 2;
        }
        // The "duration to sleep per byte" is given by the total duration / number of bytes
        let tsleep = wtime / TryInto::<u32>::try_into(wlen).unwrap();

        // Divide the window_length and window_time as long as we overflow u64::MAX
        // We will use u64 throughout the algorithm, and wan't to convert from u128 with unwrap
        while window_time.as_nanos() > u64::MAX as u128 {
            window_time /= 2;
            window_length /= 2;
        }

        // What will limit a one-shot read ? Can be window_length or bucket_size if smaller
        let stream_cap_limit = std::cmp::min(window_length, bucket_size);
        LimiterOptions {
            stream_cap_limit,
            wtime_ns: window_time.as_nanos().try_into().unwrap(),
            sleep_threshold: stream_cap_limit,
            window_length,
            window_time,
            bucket_size,
            tsleep,
            timeout: None,
        }
    }
}

impl LimiterOptions {
    /// Sets a minimal size for the IO operation to perform.
    /// The number of tokens available in order to read / write will have to be at
    /// least this size (expect there is not enough data left)
    /// Useful for TcpStream operations (Tcp operation has 60Kb of data / packet)
    pub fn set_min_operation_size(&mut self, val: u64) {
        assert_ne!(val, 0);
        assert!(
            self.bucket_size >= val,
            "Bucket size: {}, min_operation_size: {}",
            self.bucket_size,
            val
        );
        self.sleep_threshold = self.sleep_threshold.max(val);
    }

    // Sets a timeout so we can interrupt a limited stream read / write once it has
    // lasted too much time
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }
}

/// A `Limiter` is a wrapper around a stream that implement `Read` and `Write`
/// that limits the rate at which it can be read or written.
pub struct Limiter<S>
where
    S: Read + Write,
{
    pub stream: S,
    pub read_opt: Option<LimiterOptions>,
    pub write_opt: Option<LimiterOptions>,
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
    /// Create a new `Limiter` with the given options passed in parameter
    /// If an option is None, the operation will be performed on the raw stream
    pub fn new(
        stream: S,
        read_opt: Option<LimiterOptions>,
        write_opt: Option<LimiterOptions>,
    ) -> Limiter<S> {
        Limiter {
            stream,
            // Instant at which we last performed a read
            last_read_check: if read_opt.is_some() {
                Some(std::time::Instant::now())
            } else {
                None
            },
            // Instant at which we last performed a write
            last_write_check: if write_opt.is_some() {
                Some(std::time::Instant::now())
            } else {
                None
            },
            read_opt,
            write_opt,
            additionnal_tokens: (0, 0),

            // For testing and debug purposes
            #[cfg(test)]
            blocking_duration: (Duration::ZERO, Duration::ZERO),
        }
    }

    // Get the raw stream, deconstruct the Limiter struct.
    pub fn get_stream(self) -> S {
        self.stream
    }

    // Get the number of bytes available for read / write.
    fn tokens_available(&self) -> (Option<u64>, Option<u64>) {
        let read_tokens = if let Some(LimiterOptions {
            window_length,
            bucket_size,
            wtime_ns,
            ..
        }) = self.read_opt
        {
            // Get the number of nanoseconds since last read
            let lrc = match u64::try_from(self.last_read_check.unwrap().elapsed().as_nanos()) {
                Ok(n) => n,
                // Will cap the last_read_check at a duration of about 584 years
                Err(_) => u64::MAX,
            };
            if wtime_ns == 0 {
                // If we don't wait at all because of options, we can read u64::MAX bytes at once
                Some(u64::MAX)
            } else {
                // Cross product to get the number of bytes we can read
                // Add additionnal tokens we had from previous iterations
                Some(
                    std::cmp::min(lrc.saturating_mul(window_length) / wtime_ns, bucket_size)
                        .saturating_add(self.additionnal_tokens.0),
                )
            }
        } else {
            None
        };

        // Same as read operation
        let write_tokens = if let Some(LimiterOptions {
            window_length,
            bucket_size,
            wtime_ns,
            ..
        }) = self.write_opt
        {
            let lwc = match u64::try_from(self.last_write_check.unwrap().elapsed().as_nanos()) {
                Ok(n) => n,
                Err(_) => u64::MAX,
            };
            if wtime_ns == 0 {
                Some(u64::MAX)
            } else {
                Some(
                    std::cmp::min(lwc.saturating_mul(window_length) / wtime_ns, bucket_size)
                        .saturating_add(self.additionnal_tokens.1),
                )
            }
        } else {
            None
        };
        (read_tokens, write_tokens)
    }

    // Get if this Limiter limits the read or write stream (or none)
    pub fn limits(&self) -> (bool, bool) {
        (
            self.read_opt.is_some() && self.last_read_check.is_some(),
            self.write_opt.is_some() && self.last_write_check.is_some(),
        )
    }

    // Read instantly from the stream, add duration it took to the attribute for debugging
    #[cfg(test)]
    pub fn read_instant(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read_start_instant = std::time::Instant::now();
        let nb = self.stream.read(buf)?;
        self.blocking_duration.0 += read_start_instant.elapsed();
        Ok(nb)
    }

    // Read instantly from the stream
    #[cfg(not(test))]
    pub fn read_instant(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }

    // Write instantly from the stream, add duration it took to the attribute for debugging
    #[cfg(test)]
    pub fn write_instant(&mut self, buf: &[u8]) -> io::Result<usize> {
        let write_start_instant = std::time::Instant::now();
        let nb = self.stream.write(buf)?;
        self.blocking_duration.1 += write_start_instant.elapsed();
        Ok(nb)
    }

    // Write instantly from the stream
    #[cfg(not(test))]
    pub fn write_instant(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }
}

impl<S> Read for Limiter<S>
where
    S: Read + Write,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Initialize the algorithm
        let read_start = Instant::now();
        let mut read: u64 = 0;
        let mut buf_left = u64::try_from(buf.len()).expect("R buflen to u64");
        let Some(opts) = self.read_opt.as_ref() else {
            // If the stream isn't limited, read instantly instead
            return self.read_instant(buf);
        };

        while buf_left > 0 {
            // Timeout if time since start of algorithm is greater than timeout set in options
            if let Some(t) = opts.timeout {
                if read_start.elapsed() > t {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "Read timeout"));
                }
            }

            // Get the number of bytes we can read since last loop
            let nb_bytes_readable = self.tokens_available().0.unwrap().min(buf_left);
            // Get the number of bytes under which it's not worth doing a read and we need to sleep instead
            let sleep_threshold = opts.sleep_threshold.min(buf_left);

            // If it's not worth reading yet, we sleep and loop back later
            if nb_bytes_readable < sleep_threshold {
                // Check how much we need before it's worth reading
                let nb_left: u32 = sleep_threshold
                    .saturating_sub(nb_bytes_readable)
                    .try_into()
                    .expect("Read nb left > u32::MAX");

                // Compute the time required to get to the number of bytes required
                let tsleep_total = if let Some(t) = opts.timeout {
                    (opts.tsleep * nb_left).min(t.saturating_sub(read_start.elapsed()))
                } else {
                    opts.tsleep * nb_left
                };

                std::thread::sleep(tsleep_total);

                // On debug mode, we check that we have MORE bytes to read after sleep
                #[cfg(debug_assertions)]
                {
                    if nb_bytes_readable != opts.bucket_size {
                        let new_nb_bytes_readable = self.tokens_available().0.unwrap();
                        debug_assert!(new_nb_bytes_readable > nb_bytes_readable,
                            "\n{:?}\nTsleep: {:?} x {nb_left} = {:?}\nReadlimit: {}\n{nb_bytes_readable} == {new_nb_bytes_readable}",
                            self.read_opt.as_ref(),
                            opts.tsleep,
                            opts.tsleep * nb_left,
                            opts.stream_cap_limit,
                        );
                    }
                }
                continue;
            }

            // Before reading so that we don't count the time it takes to read
            self.last_read_check = Some(std::time::Instant::now());

            // Compute the indexes of the start / end on our buffer
            let read_start = usize::try_from(read).expect("R read_start to usize");
            let read_end = usize::try_from(read.saturating_add(nb_bytes_readable.min(buf_left)))
                .expect("R read_end to usize");

            // For debugging stats in tests
            #[cfg(test)]
            let read_start_instant = std::time::Instant::now();

            let read_now = u64::try_from(self.stream.read(&mut buf[read_start..read_end])?)
                .expect("R read_now to u64");

            // Add duration of the read operation in the stats for debugging in tests
            #[cfg(test)]
            {
                self.blocking_duration.0 += read_start_instant.elapsed();
            }

            // If we haven't spent all of our tokens yet, add the rest to the additionnal_tokens
            self.additionnal_tokens = (
                nb_bytes_readable.saturating_sub(read_now),
                self.additionnal_tokens.1,
            );

            read = read.saturating_add(read_now);
            buf_left = buf_left.saturating_sub(read_now);

            // If there's nothing left to read, stop the process
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
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Initialize the algorithm
        let write_start = Instant::now();
        let mut write: u64 = 0;
        let mut buf_left = u64::try_from(buf.len()).expect("W buflen to u64");
        let Some(opts) = self.write_opt.as_ref() else {
            // If the stream isn't limited, write instantly instead
            return self.write_instant(buf);
        };

        while buf_left > 0 {
            // Timeout if time since start of algorithm is greater than timeout set in options
            if let Some(t) = opts.timeout {
                if write_start.elapsed() > t {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "Write timeout"));
                }
            }

            // Get the number of bytes we can write since last loop
            let nb_bytes_writable = self.tokens_available().1.unwrap().min(buf_left);
            // Get the number of bytes under which it's not worth doing a write and we need to sleep instead
            let sleep_threshold = opts.sleep_threshold.min(buf_left);

            // If it's not worth writing yet, we sleep and loop back later
            if nb_bytes_writable < sleep_threshold {
                // Check how much we need before it's worth writing
                let nb_left: u32 = sleep_threshold
                    .saturating_sub(nb_bytes_writable)
                    .try_into()
                    .expect("Write nb left > u32::MAX");

                // Compute the time required to get to the number of bytes required
                let tsleep_total = if let Some(t) = opts.timeout {
                    (opts.tsleep * nb_left).min(t.saturating_sub(write_start.elapsed()))
                } else {
                    opts.tsleep * nb_left
                };

                std::thread::sleep(tsleep_total);

                // On debug mode, we check that we have MORE bytes to write after sleep
                #[cfg(debug_assertions)]
                {
                    if nb_bytes_writable != opts.bucket_size {
                        let new_nb_bytes_writable = self.tokens_available().1.unwrap();
                        debug_assert!(
                            new_nb_bytes_writable > nb_bytes_writable,
                            "\n{:?}\nTsleep: {:?}\nWritelimit: {}\n",
                            self.write_opt.as_ref(),
                            opts.tsleep,
                            opts.stream_cap_limit,
                        );
                    }
                }
                continue;
            }

            // Before writing so that we don't count the time it takes to write
            self.last_write_check = Some(std::time::Instant::now());

            // Compute the indexes of the start / end on our buffer
            let write_start = usize::try_from(write).expect("W write_start to usize");
            let write_end = usize::try_from(write.saturating_add(nb_bytes_writable.min(buf_left)))
                .expect("W write_end to usize");

            // For debugging stats in tests
            #[cfg(test)]
            let write_start_instant = std::time::Instant::now();

            let write_now = u64::try_from(self.stream.write(&buf[write_start..write_end])?)
                .expect("W write_now_ to u64");

            // Add duration of the write operation in the stats for debugging in tests
            #[cfg(test)]
            {
                self.blocking_duration.1 += write_start_instant.elapsed();
            }

            // If we haven't spent all of our tokens yet, add the rest to the additionnal_tokens
            self.additionnal_tokens = (
                self.additionnal_tokens.0,
                nb_bytes_writable.saturating_sub(write_now),
            );

            write = write.saturating_add(write_now);
            buf_left = buf_left.saturating_sub(write_now);

            // If there's nothing left to write, stop the process
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
