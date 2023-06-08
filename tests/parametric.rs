mod utils;

mod tests {
    use std::{time::Duration, io::{Write, Read}};
    use stream_limiter::{LimiterOptions, Limiter};
    use crate::utils::paramtests::start_parametric_test;
    use sha2::Digest;

    const INSTANT_IO_EPS : u128 = 1_500_000;

    fn get_data_hash(data: &Vec<u8>) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    fn get_random_options<R: rand::Rng>(rng: &mut R, datalen: usize) -> Option<LimiterOptions> {
        if rng.gen_bool(0.15) {
            None
        } else {
            Some(LimiterOptions::new(
                rng.gen_range((datalen / 10)..(datalen * 10)) as u128,
                Duration::from_millis(rng.gen_range(10..200)),
                rng.gen_range((datalen / 10)..(datalen * 10)),
            ))
        }
    }

    #[test]
    fn test_buffer() {
        fn paramtest_buffer<R: rand::Rng>(mut rng: R) {
            let datalen = rng.gen_range(10..1024*512);

            let outbuf = std::io::Cursor::new(vec![]);
            let wopts = get_random_options(&mut rng, datalen);
            let ropts = get_random_options(&mut rng, datalen);

            let data: Vec<u8> = (0..datalen).map(|_| rng.gen::<u8>()).collect();
            let buf = data.clone();
            let data_checksum = get_data_hash(&buf);
            
            let mut limiter = Limiter::new(
                outbuf,
                ropts.clone(),
                wopts.clone(),
            );
            println!("Writing {} data with opts {:?}", datalen, wopts);
            let now = std::time::Instant::now();
            let nwrite = limiter.write(&buf).unwrap();
            let elapsed = now.elapsed();
            assert_eq!(nwrite, datalen);
            if let Some(ref opts) = wopts {
                let rate = opts.window_length.min(opts.bucket_size as u128);
                if datalen as u128 > rate {
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
            let mut limiter = Limiter::new(
                std::io::Cursor::new(read_buf),
                ropts.clone(),
                wopts.clone(),
            );
            println!("Reading {} data with opts {:?}", datalen, ropts);
            let now = std::time::Instant::now();
            let nread = limiter.read(buf.as_mut_slice()).unwrap();
            let elapsed = now.elapsed();
            assert_eq!(nread, datalen);
            if let Some(ref opts) = ropts {
                let rate = opts.window_length.min(opts.bucket_size as u128);
                if datalen as u128 > rate {
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
        start_parametric_test(3_000_000, vec![
            14398057406427516238,
            13640999559978117227,
        ], paramtest_buffer);
    }

    #[test]
    fn test_tcp() {
        use std::net::{TcpStream, TcpListener};

        fn paramtest_tcp<R: rand::Rng>(mut rng: R) {
            let datalen = rng.gen_range(10..1024*512);
            let wopts_connector = get_random_options(&mut rng, datalen);
            let wopts_listener = get_random_options(&mut rng, datalen);
            let ropts_connector = get_random_options(&mut rng, datalen);
            let ropts_listener = get_random_options(&mut rng, datalen);
            let data: Vec<u8> = (0..datalen).map(|_| rng.gen::<u8>()).collect();
            let data_c = data.clone();
            let datahash = get_data_hash(&data);
            let port = 10000 + rng.gen_range(0..(u16::MAX - 10000));

            let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();

            std::thread::spawn(move || {
                let stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
                let mut limiter = Limiter::new(
                    stream,
                    ropts_connector.clone(),
                    wopts_connector.clone(),
                );

                let now = std::time::Instant::now();
                limiter.write_all(&data_c).unwrap();
                let elapsed = now.elapsed();
                if let Some(opts) = wopts_connector {
                    let rate = opts.window_length.min(opts.bucket_size as u128);
                    if datalen as u128 > rate {
                        assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                    } else {
                        assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                    }
                }

                let mut buf = vec![0; datalen];
                let now = std::time::Instant::now();
                limiter.read_exact(&mut buf).unwrap();
                let elapsed = now.elapsed();
                assert_eq!(get_data_hash(&buf), datahash);
                if let Some(ref opts) = ropts_connector {
                    let rate = opts.window_length.min(opts.bucket_size as u128);
                    if datalen as u128 > rate {
                        assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                    } else {
                        assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                    }
                }
            });

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
                if let Some(ref opts) = ropts_listener {
                    let rate = opts.window_length.min(opts.bucket_size as u128);
                    if datalen as u128 > rate {
                        assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                    } else {
                        assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                    }
                }

                let now = std::time::Instant::now();
                limiter.write_all(&data).unwrap();
                let elapsed = now.elapsed();
                if let Some(opts) = wopts_listener {
                    let rate = opts.window_length.min(opts.bucket_size as u128);
                    if datalen as u128 > rate {
                        assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                    } else {
                        assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                    }
                }
                break;
            }
        }
        start_parametric_test(3_000_000, vec![
        ], paramtest_tcp);
    }
}
