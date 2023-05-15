mod utils;

// TODO    Create parametric tests
mod tests {
    use std::{fs::File, io::Read, time::Duration};

    use super::utils::{assert_checksum, FILE_BIG, FILE_LITTLE, FILE_TEST};
    use stream_limiter::Limiter;

    #[test]
    fn one_byte_each_second() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(file, Some((1, Duration::from_secs(1), 10)), None);
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 10];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 9);
    }

    #[test]
    fn one_byte_each_two_hundreds_fifty_millis() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(file, Some((1, Duration::from_millis(250), 10)), None);
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 10];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 2);
    }

    #[test]
    fn two_byte_each_second() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(file, Some((2, Duration::from_secs(1), 10)), None);
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 10];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 4);
    }

    #[test]
    fn read_instant() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(file, Some((10, Duration::from_secs(1), 10)), None);
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 10];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 0);
    }

    #[test]
    fn read_instant_on_bigger_buffer() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(file, Some((100, Duration::from_secs(1), 1000)), None);
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
        let mut limiter = Limiter::new(file, Some((1, Duration::from_secs(1), 9)), None);
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
        let mut limiter = Limiter::new(file, Some((1, Duration::from_secs(1), 5)), None);
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = Vec::with_capacity(5);
        limiter.read_to_end(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 5);
        assert_checksum(&buf, &FILE_LITTLE);
    }

    #[test]
    fn tenko_limit() {
        let file = File::open("tests/resources/big.txt").unwrap();
        let mut limiter = Limiter::new(file, Some((10 * 1024, Duration::from_secs(1), 12 * 1024)), None);
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 11 * 1024];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 1);
        assert_checksum(&buf, &FILE_BIG);
    }

    #[test]
    fn splitted_read() {
        let file = File::open("tests/resources/big.txt").unwrap();
        let mut limiter = Limiter::new(
            file,
            Some((11,
                Duration::from_nanos((1000 * 1000 * 1000) / 1024),
                12 * 1024
            )),
            None
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
        assert_eq!(now.elapsed().as_secs(), 1);
        assert_checksum(&res_buffer, &FILE_BIG);
    }

    #[test]
    fn test_bucket_full() {
        let file = File::open("tests/resources/test.txt").unwrap();
        let mut limiter = Limiter::new(file, Some((1024, Duration::from_secs(1), 10)), None);
        assert!(limiter.limits().0);
        let now = std::time::Instant::now();
        let mut buf = [0u8; 15];
        limiter.read(&mut buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 1);
    }

    // TODO    Add test changing the bucket size between 2 reads
}
