mod utils;

mod tests {
    use stream_limiter::{Limiter, LimiterOptions};
    use std::net::{TcpListener, TcpStream};
    use std::io::{Read, Write};
    use std::time::Duration;

    #[test]
    fn test_limit_read() {
        let listener = TcpListener::bind("127.0.0.1:34254").unwrap();
        std::thread::spawn(|| {
            println!("W] Connecting...");
            let mut stream = TcpStream::connect("127.0.0.1:34254").unwrap();
            println!("W] Writing ...");
            stream.write(&[42u8; 10]).unwrap();
            println!("W] OK");
        });
        println!("R] Listening ...");
        for stream in listener.incoming() {
            println!("R] Stream {:?} connected", stream);
            let mut limiter = Limiter::new(
                stream.unwrap(),
                Some(LimiterOptions::new(9, Duration::from_secs(1), 10)),
                None,
            );
            println!("R] Reading with limitation");
            let mut buffer = [0u8; 10];
            let now = std::time::Instant::now();
            limiter.read(&mut buffer).unwrap();
            println!("R] Result: {:?}", buffer);
            assert_eq!(buffer, [42; 10]);
            assert_eq!(now.elapsed().as_secs(), 1);
            break;
        }
    }

    #[test]
    fn test_limit_write() {
        let listener = TcpListener::bind("127.0.0.1:34255").unwrap();
        std::thread::spawn(|| {
            println!("W] Connecting...");
            let stream = TcpStream::connect("127.0.0.1:34255").unwrap();
            let mut limiter = Limiter::new(
                stream,
                None,
                Some(LimiterOptions::new(9, Duration::from_secs(1), 10)),
            );
            println!("W] Writing ...");
            limiter.write(&[42u8; 10]).unwrap();
            println!("W] OK");
        });
        println!("R] Listening ...");
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            println!("R] Stream {:?} connected", stream);
            println!("R] Reading with limitation");
            let mut buffer = Vec::new();
            let now = std::time::Instant::now();
            stream.read_to_end(&mut buffer).unwrap();

            println!("R] Result: {:?}", buffer);
            assert_eq!(buffer, [42; 10]);
            assert_eq!(now.elapsed().as_secs(), 1);
            break;
        }
    }

    #[test]
    fn test_limit_both() {
        let listener = TcpListener::bind("127.0.0.1:34256").unwrap();
        std::thread::spawn(|| {
            println!("W] Connecting...");
            let stream = TcpStream::connect("127.0.0.1:34256").unwrap();
            let mut limiter = Limiter::new(
                stream,
                None,
                Some(LimiterOptions::new(9, Duration::from_secs(1), 10)),
            );
            println!("W] Writing ...");
            limiter.write_all(&[42u8; 10]).unwrap();
            println!("W] OK");
        });
        println!("R] Listening ...");
        for stream in listener.incoming() {
            println!("R] Stream {:?} connected", stream);
            println!("R] Reading with limitation");
            let mut buffer = Vec::new();
            let now = std::time::Instant::now();
            let mut limiter = Limiter::new(
                stream.unwrap(),
                Some(LimiterOptions::new(9, Duration::from_secs(1), 10)),
                None,
            );
            limiter.read_to_end(&mut buffer).unwrap();

            println!("R] Result: {:?}", buffer);
            assert_eq!(buffer, [42; 10]);
            assert_eq!(now.elapsed().as_secs(), 1);
            break;
        }
    }

    #[test]
    fn test_peak_both() {
        let listener = TcpListener::bind("127.0.0.1:34257").unwrap();
        std::thread::spawn(|| {
            println!("W] Connecting...");
            let stream = TcpStream::connect("127.0.0.1:34257").unwrap();
            let mut limiter = Limiter::new(
                stream,
                None,
                Some(LimiterOptions::new(10, Duration::from_secs(1), 10)),
            );
            println!("W] Writing ...");
            limiter.write_all(&[42u8; 10]).unwrap();
            println!("W] OK");
        });
        println!("R] Listening ...");
        for stream in listener.incoming() {
            println!("R] Stream {:?} connected", stream);
            println!("R] Reading with limitation");
            let mut buffer = Vec::new();
            let now = std::time::Instant::now();
            let mut limiter = Limiter::new(
                stream.unwrap(),
                Some(LimiterOptions::new(10, Duration::from_secs(1), 10)),
                None,
            );
            limiter.read_to_end(&mut buffer).unwrap();

            println!("R] Result: {:?}", buffer);
            assert_eq!(buffer, [42; 10]);
            assert_eq!(now.elapsed().as_secs(), 1);
            break;
        }
    }
}
