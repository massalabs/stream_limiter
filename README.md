# stream_limiter

Synchronously speed-limiting streams.

This crate provides a `Limiter` struct that can be used to limit the rate at which a stream can be read or written.

This crate is based on the token bucket algorithm. When we want to read data and we are rate limited the packet aren't drop but we sleep.

Example:
```rust
    use stream_limiter::Limiter;
    use std::time::Duration;
    use std::io::prelude::*;
    use std::fs::File;

    let mut file = File::open("tests/resources/test.txt").unwrap();
    let mut limiter = Limiter::new(file, 1, Duration::from_secs(1));
    let mut buf = [0u8; 10];
    let now = std::time::Instant::now();
    limiter.read(&mut buf).unwrap();
    assert_eq!(now.elapsed().as_secs(), 9);
```