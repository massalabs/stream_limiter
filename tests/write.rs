mod tests {
    use std::{io::Write, time::Duration};

    use stream_limiter::Limiter;
    use tempfile::tempfile;

    #[test]
    fn one_byte_each_second() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 1, Duration::from_secs(1), 10);
        let now = std::time::Instant::now();
        let buf = [0u8; 10];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 9);
    }

    #[test]
    fn one_byte_each_two_hundreds_fifty_millis() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 1, Duration::from_millis(250), 10);
        let now = std::time::Instant::now();
        let buf = [0u8; 10];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 2);
    }

    #[test]
    fn two_byte_each_second() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 2, Duration::from_secs(1), 10);
        let now = std::time::Instant::now();
        let buf = [0u8; 10];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 4);
    }

    #[test]
    fn write_instant() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 10, Duration::from_secs(1), 10);
        let now = std::time::Instant::now();
        let buf = [0u8; 10];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 0);
    }

    #[test]
    fn test_burst() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 1, Duration::from_secs(1), 1);
        // Write a first byte of 1 byte. Should be instant
        let now = std::time::Instant::now();
        let buf = [0u8; 1];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 0);

        std::thread::sleep(Duration::from_secs(9));

        // Write a second byte of 9 bytes. Should be instant because we waited above
        let now = std::time::Instant::now();
        let buf = [0u8; 9];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 0);
    }

    #[test]
    fn tenko_limit() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 10 * 1024, Duration::from_secs(1), 12*1024);
        let now = std::time::Instant::now();
        let buf = [0u8; 11 * 1024];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 1);
    }

    #[test]
    fn splitted_read() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 11, Duration::from_nanos((1000 * 1000 * 1000) / 1024), 12*1024);
        let now = std::time::Instant::now();
        let buf = [0u8; 8];
        limiter.write(&buf).unwrap();
        let buf = [0u8; (11 * 1024) - 8];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 1);
    }
    #[test]
    fn write_bucket_full() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 1024, Duration::from_secs(1), 10);
        let now = std::time::Instant::now();
        let buf = [0u8; 15];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 1);
    }
}
