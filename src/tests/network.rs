use crate::{Limiter, LimiterOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

#[test]
fn test_limit_read() {
    const WINDOW_RATE: usize = 9;
    const BUFFER_SIZE: usize = 2 * WINDOW_RATE;
    let listener = TcpListener::bind("127.0.0.1:34254").unwrap();
    std::thread::spawn(|| {
        println!("W] Connecting...");
        let mut stream = TcpStream::connect("127.0.0.1:34254").unwrap();
        println!("W] Writing ...");
        stream.write(&[42u8; BUFFER_SIZE]).unwrap();
        println!("W] OK");
    });
    println!("R] Listening ...");
    for stream in listener.incoming() {
        println!("R] Stream {:?} connected", stream);
        let mut limiter = Limiter::new(
            stream.unwrap(),
            Some(LimiterOptions::new(
                WINDOW_RATE as u64,
                Duration::from_secs(1),
                (WINDOW_RATE + 1) as u64,
            )),
            None,
        );
        println!("R] Reading with limitation");
        let mut buffer = [0u8; BUFFER_SIZE];
        let now = std::time::Instant::now();
        limiter.read(&mut buffer).unwrap();
        println!("R] Result: {:?} (len {})", buffer, buffer.len());
        assert_eq!(buffer, [42; BUFFER_SIZE]);
        assert_eq!(now.elapsed().as_secs(), 2, "{:?}", now.elapsed());
        break;
    }
}

#[test]
fn test_limit_write() {
    const WINDOW_RATE: usize = 9;
    const BUFFER_SIZE: usize = 2 * WINDOW_RATE;
    let listener = TcpListener::bind("127.0.0.1:34255").unwrap();
    std::thread::spawn(|| {
        println!("W] Connecting...");
        let stream = TcpStream::connect("127.0.0.1:34255").unwrap();
        let mut limiter = Limiter::new(
            stream,
            None,
            Some(LimiterOptions::new(
                WINDOW_RATE as u64,
                Duration::from_secs(1),
                (WINDOW_RATE + 1) as u64,
            )),
        );
        println!("W] Writing ...");
        limiter.write(&[42u8; BUFFER_SIZE]).unwrap();
        println!("W] OK");
    });
    println!("R] Listening ...");
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        println!("R] Stream {:?} connected", stream);
        println!("R] Reading with limitation");
        let mut buffer = [0; BUFFER_SIZE];
        let now = std::time::Instant::now();
        stream.read_exact(&mut buffer).unwrap();

        println!("R] Result: {:?} (len {})", buffer, buffer.len());
        assert_eq!(buffer, [42; BUFFER_SIZE]);
        assert_eq!(now.elapsed().as_secs(), 2);
        break;
    }
}

#[test]
fn test_limit_both() {
    const WINDOW_RATE: usize = 9;
    const BUFFER_SIZE: usize = 2 * WINDOW_RATE;
    let listener = TcpListener::bind("127.0.0.1:34256").unwrap();
    std::thread::spawn(|| {
        println!("W] Connecting...");
        let stream = TcpStream::connect("127.0.0.1:34256").unwrap();
        let mut limiter = Limiter::new(
            stream,
            None,
            Some(LimiterOptions::new(
                WINDOW_RATE as u64,
                Duration::from_secs(1),
                (WINDOW_RATE + 1) as u64,
            )),
        );
        println!("W] Writing ...");
        limiter.write_all(&[42u8; BUFFER_SIZE]).unwrap();
        println!("W] OK");
    });
    println!("R] Listening ...");
    for stream in listener.incoming() {
        println!("R] Stream {:?} connected", stream);
        println!("R] Reading with limitation");
        let mut buffer = [0; BUFFER_SIZE];
        let now = std::time::Instant::now();
        let mut limiter = Limiter::new(
            stream.unwrap(),
            Some(LimiterOptions::new(
                WINDOW_RATE as u64,
                Duration::from_secs(1),
                (WINDOW_RATE + 1) as u64,
            )),
            None,
        );
        limiter.read_exact(&mut buffer).unwrap();

        println!("R] Result: {:?} (len {})", buffer, buffer.len());
        assert_eq!(buffer, [42; BUFFER_SIZE]);
        assert_eq!(now.elapsed().as_secs(), 2);
        break;
    }
}

#[test]
fn test_no_limit() {
    const WINDOW_RATE: usize = 9;
    const BUFFER_SIZE: usize = WINDOW_RATE;
    let listener = TcpListener::bind("127.0.0.1:34257").unwrap();
    std::thread::spawn(|| {
        println!("W] Connecting...");
        let stream = TcpStream::connect("127.0.0.1:34257").unwrap();
        let mut limiter = Limiter::new(stream, None, None);
        println!("W] Writing ...");
        limiter.write_all(&[42u8; BUFFER_SIZE]).unwrap();
        println!("W] OK");
    });
    println!("R] Listening ...");
    for stream in listener.incoming() {
        println!("R] Stream {:?} connected", stream);
        println!("R] Reading with limitation");
        let mut buffer = [0; BUFFER_SIZE];
        let now = std::time::Instant::now();
        let mut limiter = Limiter::new(stream.unwrap(), None, None);
        limiter.read_exact(&mut buffer).unwrap();

        println!(
            "R] Result: {:?} (len {}), in {:?}",
            buffer,
            buffer.len(),
            now.elapsed()
        );
        assert_eq!(now.elapsed().as_secs(), 0, "took {:?}", now.elapsed());
        assert_eq!(buffer, [42; BUFFER_SIZE]);
        break;
    }
}
