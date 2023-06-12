mod utils;

mod tests {
    use crate::utils::paramtests::start_parametric_test;
    use sha2::Digest;
    use std::{
        io::{Read, Write},
        time::Duration,
    };
    use stream_limiter::{Limiter, LimiterOptions};

    const INSTANT_IO_EPS: u128 = 1_500_000;
    const DATALEN_DIVIDER: usize = 5;

    fn get_data_hash(data: &Vec<u8>) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    fn get_random_options<R: rand::Rng>(rng: &mut R, datalen: usize) -> Option<LimiterOptions> {
        if rng.gen_bool(0.08) {
            None
        } else {
            Some(LimiterOptions::new(
                rng.gen_range((datalen / DATALEN_DIVIDER)..(datalen * DATALEN_DIVIDER)) as u64,
                Duration::from_millis(
                    rng.gen_range(DATALEN_DIVIDER..(1000 / DATALEN_DIVIDER)) as u64
                ),
                rng.gen_range((datalen / DATALEN_DIVIDER)..(datalen * DATALEN_DIVIDER)) as u64,
            ))
        }
    }

    #[test]
    fn test_buffer() {
        fn paramtest_buffer<R: rand::Rng>(mut rng: R) {
            let datalen = rng.gen_range(10..1024 * 512);

            let outbuf = std::io::Cursor::new(vec![]);
            let wopts: Option<LimiterOptions> = get_random_options(&mut rng, datalen);
            let ropts = get_random_options(&mut rng, datalen);

            let data: Vec<u8> = (0..datalen).map(|_| rng.gen::<u8>()).collect();
            let buf = data.clone();
            let data_checksum = get_data_hash(&buf);

            let mut limiter = Limiter::new(outbuf, ropts.clone(), wopts.clone());
            let now = std::time::Instant::now();
            let nwrite = limiter.write(&buf).unwrap();
            let elapsed = now.elapsed();
            assert_eq!(nwrite, datalen);
            if let Some(ref opts) = wopts {
                let rate = opts.window_length.min(opts.bucket_size);
                if (datalen as u64) > rate {
                    assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                } else {
                    assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                }
            } else {
                assert!(elapsed.as_nanos() < INSTANT_IO_EPS);
            }

            assert_eq!(get_data_hash(limiter.stream.get_ref()), data_checksum);

            let read_buf = limiter.stream.into_inner();
            let mut buf = vec![0; datalen];
            let mut limiter = Limiter::new(std::io::Cursor::new(read_buf), ropts.clone(), wopts);
            let now = std::time::Instant::now();
            let nread = limiter.read(buf.as_mut_slice()).unwrap();
            let elapsed = now.elapsed();
            assert_eq!(nread, datalen);
            if let Some(ref opts) = ropts {
                let rate = opts.window_length.min(opts.bucket_size);
                if datalen as u64 > rate {
                    assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                } else {
                    assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                }
            } else {
                assert!(elapsed.as_nanos() < INSTANT_IO_EPS);
            }

            assert_eq!(get_data_hash(&buf), data_checksum);
            assert_eq!(&data, &buf);
        }
        start_parametric_test(
            100,
            vec![14398057406427516238, 13640999559978117227],
            paramtest_buffer,
        );
    }

    #[test]
    fn test_tcp() {
        use std::net::{TcpListener, TcpStream};

        fn paramtest_tcp<R: rand::Rng>(mut rng: R) {
            let datalen = rng.gen_range(10..1024 * 512);
            // println!("Wopts connector");
            let wopts_connector: Option<LimiterOptions> = get_random_options(&mut rng, datalen);
            // println!("Wopts listener");
            let wopts_listener: Option<LimiterOptions> = get_random_options(&mut rng, datalen);
            // println!("Ropts connector");
            let ropts_connector = get_random_options(&mut rng, datalen);
            // println!("Ropts listener");
            let ropts_listener = get_random_options(&mut rng, datalen);
            let data: Vec<u8> = (0..datalen).map(|_| rng.gen::<u8>()).collect();
            let data_c = data.clone();
            let datahash = get_data_hash(&data);
            let port = 10000 + rng.gen_range(0..(u16::MAX - 10000));

            let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();

            let connector = std::thread::spawn(move || {
                let stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
                let mut limiter =
                    Limiter::new(stream, ropts_connector.clone(), wopts_connector.clone());

                let now = std::time::Instant::now();
                limiter.write_all(&data_c).unwrap();
                let elapsed = now.elapsed();
                if let Some(ref opts) = wopts_connector {
                    let rate = opts.window_length.min(opts.bucket_size);
                    if datalen as u64 > rate {
                        assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                    }
                     else {
                        assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                    }
                }

                let mut limiter = Limiter::new(
                    limiter.stream,
                    ropts_connector.clone(),
                    wopts_connector.clone(),
                );
                let mut buf = vec![0; datalen];
                assert_ne!(get_data_hash(&buf), datahash);
                let now = std::time::Instant::now();
                limiter.read_exact(&mut buf).unwrap();
                let elapsed = now.elapsed();
                assert_eq!(get_data_hash(&buf), datahash);
                if let Some(ref opts) = ropts_connector {
                    let rate = opts.window_length.min(opts.bucket_size);
                    if datalen as u64 > rate {
                        assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                    }
                    else {
                        println!("{:?} (datalen {datalen})", opts);
                        println!("{:?} <= {:?} ?", elapsed, opts.window_time);
                        assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                    }
                }
            });
            std::thread::sleep(Duration::from_millis(50));

            for stream in listener.incoming() {
                let mut limiter = Limiter::new(
                    stream.unwrap(),
                    ropts_listener.clone(),
                    wopts_listener.clone(),
                );

                let mut buf = vec![0; datalen];
                let now = std::time::Instant::now();
                limiter.read_exact(&mut buf).unwrap();
                let elapsed = now.elapsed();
                assert_eq!(get_data_hash(&buf), datahash);
                if let Some(ref opts) = ropts_listener {
                    let rate = opts.window_length.min(opts.bucket_size);
                    if datalen as u64 > rate {
                        assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                    }
                    else {
                        println!("{:?} <= {:?} ?", elapsed, opts.window_time);
                        assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                    }
                }

                let mut limiter =
                    Limiter::new(limiter.stream, ropts_listener, wopts_listener.clone());
                let now = std::time::Instant::now();
                limiter.write_all(&data).unwrap();
                let elapsed = now.elapsed();
                if let Some(opts) = wopts_listener {
                    let rate = opts.window_length.min(opts.bucket_size);
                    if datalen as u64 > rate {
                        assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                    }
                    else {
                        assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                    }
                }
                break;
            }
            assert!(connector.join().is_ok());
        }
        start_parametric_test(
            100,
            vec![
                3911014536179701959,
                2770066496784563521,
                16118644738678043134,
                15039019555209573434,
                18348045085902583881,
            ],
            paramtest_tcp,
        );
    }
}
