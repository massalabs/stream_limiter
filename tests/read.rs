mod utils;

// TODO    Create parametric tests
mod tests {
    use std::{fs::File, io::Read, time::Duration};

    use super::utils::{assert_checksum, FILE_BIG, FILE_LITTLE, FILE_TEST};
    use stream_limiter::{Limiter, LimiterOptions};

    #[test]
    fn one_byte_each_second() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(1, Duration::from_secs(1), 10)),
            None,
        );
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 10];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 9);
    }

    #[test]
    fn one_byte_each_two_hundreds_fifty_millis() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(1, Duration::from_millis(250), 10)),
            None,
        );
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 10];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 2);
    }

    #[test]
    fn two_byte_each_second() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(2, Duration::from_secs(1), 10)),
            None,
        );
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 10];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 4);
    }

    #[test]
    fn read_instant() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(10, Duration::from_secs(1), 10)),
            None,
        );
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 10];
        limiter.read(&mut buf).unwrap();
        assert!(now.elapsed().as_millis() < 1, "{:?}", now.elapsed());
    }

    #[test]
    fn read_instant_on_bigger_buffer() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(1000, Duration::from_secs(1), 1000)),
            None,
        );
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 1000];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 0);
        assert_checksum(&buf, &FILE_TEST);
    }

    #[test]
    fn test_burst() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(1, Duration::from_secs(1), 9)),
            None,
        );
        assert!(limiter.limits().0);
        // Read a first byte of 1 byte. Should be instant
        let now = std::time::Instant::now();
        let mut buf = [0u8; 1];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 0);

        std::thread::sleep(Duration::from_secs(9));

        // Read a second byte of 9 bytes. Should be instant because we waited above
        let now = std::time::Instant::now();
        let mut buf = [0u8; 9];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 0);
    }

    #[test]
    fn read_the_whole_file() {
        let file = File::open("tests/resources/little.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(1, Duration::from_secs(1), 5)),
            None,
        );
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = Vec::with_capacity(5);
        limiter.read_to_end(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 5);
        assert_checksum(&buf, &FILE_LITTLE);
    }

    #[test]
    fn oneko_limit() {
        let file = File::open("tests/resources/big.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(
                1 * 1024,
                Duration::from_secs(1),
                12 * 1024,
            )),
            None,
        );
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 11 * 1024];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 10, "{:?}", now.elapsed());
        assert_checksum(&buf, &FILE_BIG);
    }

    #[test]
    fn splitted_read() {
        let file = File::open("tests/resources/big.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(
                10,
                Duration::from_secs(1) / 1024,
                12,
            )),
            None,
        );
        assert!(limiter.limits().0);

        let now = std::time::Instant::now();
        let mut res_buffer = Vec::new();

        let mut buf = [0u8; 8];
        limiter.read(&mut buf).unwrap();
        res_buffer.extend_from_slice(&buf);

        let mut buf = [0u8; (11 * 1024) - 8];
        limiter.read(&mut buf).unwrap();
        res_buffer.extend_from_slice(&buf);

        assert_eq!(now.elapsed().as_secs(), 1, "{:?}", now.elapsed());
        assert_checksum(&res_buffer, &FILE_BIG);
    }

    #[test]
    fn test_bucket_full() {
        let file = File::open("tests/resources/big.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some(LimiterOptions::new(100, Duration::from_secs(1), 10)),
            None,
        );
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();

        // 100 bytes with read peak
        let mut buf = [0u8; 100];
        limiter.read(&mut buf).unwrap();

        // Fill the bucket (will only read 10 bytes after that)
        std::thread::sleep(Duration::from_secs(1));
        // 10 bytes from bucket + 100 bytes / sec -> 1s to read 110 bytes
        let mut buf = [0u8; 110];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 2, "{:?}", now.elapsed());
    }

    // TODO    Add test changing the bucket size between 2 reads
}
