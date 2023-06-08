mod utils;

mod tests {
    use stream_limiter::{Limiter, LimiterOptions};
    use std::net::{TcpListener, TcpStream};
    use std::io::{Read, Write};
    use std::time::Duration;

    use crate::utils::paramtests::start_parametric_test;

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
            println!("R] Result: {:?} (len {})", buffer, buffer.len());
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
            let mut buffer = [0; 10];
            let now = std::time::Instant::now();
            stream.read_exact(&mut buffer).unwrap();

            println!("R] Result: {:?} (len {})", buffer, buffer.len());
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
            let mut buffer = [0; 10];
            let now = std::time::Instant::now();
            let mut limiter = Limiter::new(
                stream.unwrap(),
                Some(LimiterOptions::new(9, Duration::from_secs(1), 10)),
                None,
            );
            limiter.read_exact(&mut buffer).unwrap();

            println!("R] Result: {:?} (len {})", buffer, buffer.len());
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
            let mut buffer = [0; 10];
            let now = std::time::Instant::now();
            let mut limiter = Limiter::new(
                stream.unwrap(),
                Some(LimiterOptions::new(10, Duration::from_secs(1), 10)),
                None,
            );
            limiter.read_exact(&mut buffer).unwrap();

            println!("R] Result: {:?} (len {})", buffer, buffer.len());
            assert_eq!(buffer, [42; 10]);
            assert_eq!(now.elapsed().as_secs(), 0);
            break;
        }
    }

    const TWAIT_DIFF_PCT : f64 = 0.05;

    #[test]
    fn test_limit_speed() {
        fn paramtest_limit_speed<R: rand::Rng>(mut rng: R) {
            let datalen = rng.gen_range(0..1024*512);
            let misc_data: Vec<u8> = (0..datalen)
                .map(|_| rng.gen::<u8>()).collect();
            let misc_data2 = misc_data.clone();
            let opts = LimiterOptions::new(
                rng.gen_range(124..(1024*10)),
                Duration::from_millis(rng.gen_range(0..40000)),
                rng.gen_range(124..(1024*10)),
            );
            let opts2 = opts.clone();
            let mut port = 34300 + rng.gen_range(0..1400);
            let twait = opts2.get_tx_time(datalen);
            if twait.as_secs() > 1 {
                println!("Skipping test: would take {:?} to complete", twait);
                return;
            }

            let listener;
            loop {
                match TcpListener::bind(format!("127.0.0.1:{port}")) {
                    Ok(l) => listener = l,
                    Err(e) => {
                        println!("Error while connecting: {:?}", e);
                        port += 1;
                        continue;
                    },
                }
                break;
            }
            std::thread::spawn(move || {
                let stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
                let mut limiter = Limiter::new(
                    stream,
                    Some(opts.clone()),
                    Some(opts),
                );
                limiter.write_all(&misc_data).unwrap();
            });
            for stream in listener.incoming() {
                let mut buffer = vec![0; datalen];
                let now = std::time::Instant::now();
                let mut limiter = Limiter::new(
                    stream.unwrap(),
                    Some(opts2.clone()),
                    Some(opts2),
                );
                limiter.read_exact(buffer.as_mut_slice()).unwrap();

                let elapsed = now.elapsed();
                assert_eq!(buffer, misc_data2);
                let twait_diff = elapsed.as_nanos().abs_diff(twait.as_nanos());
                let twait_diff_pct = (twait_diff as f64) / (twait.as_nanos() as f64);
                println!("{}/{} data in {:?} (expected {:?}), diff: {:?} ({:.2} %)",
                    buffer.len(),
                    datalen,
                    elapsed,
                    twait,
                    Duration::from_nanos(twait_diff as u64),
                    twait_diff_pct,
                );
                assert!(twait_diff_pct < TWAIT_DIFF_PCT);
                break;
            }
        }
        start_parametric_test(30000, vec![], paramtest_limit_speed);
    }
}
