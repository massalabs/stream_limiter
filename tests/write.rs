mod tests {
    use std::{io::Write, time::Duration};

    use stream_limiter::Limiter;
    use tempfile::tempfile;

    #[test]
    fn one_byte_each_second() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 1, Duration::from_secs(1));
        let now = std::time::Instant::now();
        let buf = [0u8; 10];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 9);
    }

    #[test]
    fn one_byte_each_two_hundreds_fifty_millis() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 1, Duration::from_millis(250));
        let now = std::time::Instant::now();
        let buf = [0u8; 10];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 2);
    }

    #[test]
    fn two_byte_each_second() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 2, Duration::from_secs(1));
        let now = std::time::Instant::now();
        let buf = [0u8; 10];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 4);
    }

    #[test]
    fn write_instant() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 10, Duration::from_secs(1));
        let now = std::time::Instant::now();
        let buf = [0u8; 10];
        limiter.write(&buf).unwrap();
        assert_eq!(now.elapsed().as_secs(), 0);
    }

    #[test]
    fn test_burst() {
        let file = tempfile().unwrap();
        let mut limiter = Limiter::new(file, 1, Duration::from_secs(1));
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
}
