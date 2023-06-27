mod utils;

mod tests {

    use crate::utils::paramtests::start_parametric_test;
    use sha2::Digest;
    use std::{
        io::{Read, Write},
        sync::{Arc, Barrier},
        time::Duration,
    };
    use stream_limiter::{Limiter, LimiterOptions};

    const INSTANT_IO_EPS: u128 = 1_500_000;
    const DATALEN_DIVIDER: usize = 5;
    const ALLOWED_PERC_DIFF: f64 = 0.01;

    fn assert_rate_limited(
        idx: &'static str,
        opts: &Option<LimiterOptions>,
        datalen: usize,
        elapsed: Duration,
    ) {
        println!("{idx} Elapsed: {elapsed:?}, datalen: {datalen}, opts: {opts:?}");
        if let Some(opts) = opts {
            let elapsedn = elapsed.as_nanos();
            let wtime = opts.window_time.as_nanos();
            if (datalen as u64) > opts.window_length {
                if elapsedn <= wtime {
                    assert!((wtime - elapsedn) as f64 / (wtime as f64) < ALLOWED_PERC_DIFF,
                        "{idx}| Elapsed {:?} <= Window time {:?} (with {}% margin)",
                        elapsed, opts.window_time,
                        ALLOWED_PERC_DIFF * 100.0,
                    );
                }
            } else {
                if elapsedn > wtime {
                    assert!((elapsedn - wtime) as f64 / (wtime as f64) < ALLOWED_PERC_DIFF,
                        "{idx}| Elapsed {:?} > Window time {:?} (with {}% margin)",
                        elapsed, opts.window_time,
                        ALLOWED_PERC_DIFF * 100.0,
                    );
                }
            }
        } else {
            assert!(
                elapsed.as_nanos() < INSTANT_IO_EPS,
                "{idx}| {:?} (instant operation)",
                elapsed
            );
        }
    }

    fn get_data_hash(data: &Vec<u8>) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    fn get_random_options<R: rand::Rng>(
        rng: &mut R,
        datalen: usize,
        min_op_size: u64,
    ) -> Option<LimiterOptions> {
        if rng.gen_bool(0.08) {
            None
        } else {
            let mut opts = LimiterOptions::new(
                rng.gen_range((datalen / DATALEN_DIVIDER)..(datalen * DATALEN_DIVIDER)) as u64,
                Duration::from_millis(
                    rng.gen_range(DATALEN_DIVIDER..(1000 / DATALEN_DIVIDER)) as u64
                ),
                rng.gen_range((datalen / DATALEN_DIVIDER).max(min_op_size as usize)..(datalen * DATALEN_DIVIDER).max(min_op_size as usize)) as u64,
            );
            opts.set_min_operation_size(min_op_size);
            Some(opts)
        }
    }

    #[test]
    fn test_buffer() {
        fn paramtest_buffer<R: rand::Rng>(mut rng: R) {
            let datalen = rng.gen_range(10..1024 * 512);

            let outbuf = std::io::Cursor::new(vec![]);
            let wopts: Option<LimiterOptions> = get_random_options(&mut rng, datalen, 1);
            let ropts = get_random_options(&mut rng, datalen, 1);

            let data: Vec<u8> = (0..datalen).map(|_| rng.gen::<u8>()).collect();
            let buf = data.clone();
            let data_checksum = get_data_hash(&buf);

            let mut limiter = Limiter::new(outbuf, ropts.clone(), wopts.clone());
            let now = std::time::Instant::now();
            let nwrite = limiter.write(&buf).unwrap();
            let elapsed = now.elapsed();
            assert_eq!(nwrite, datalen);
            assert_rate_limited("BW", &wopts, datalen, elapsed);
            assert_eq!(get_data_hash(limiter.stream.get_ref()), data_checksum);

            println!("\n\n");
            let read_buf = limiter.stream.into_inner();
            let mut buf = vec![0; datalen];
            let mut limiter = Limiter::new(std::io::Cursor::new(read_buf), ropts.clone(), wopts);
            let now = std::time::Instant::now();
            let nread = limiter.read(buf.as_mut_slice()).unwrap();
            let elapsed = now.elapsed();
            assert_eq!(nread, datalen);
            assert_rate_limited("BR", &ropts, datalen, elapsed);
            assert_eq!(get_data_hash(&buf), data_checksum);
            assert_eq!(&data, &buf);
        }

        start_parametric_test(
            1000000,
            vec![
                15874969967965961473,
            ],
            paramtest_buffer,
        );
    }

    #[test]
    fn test_tcp() {
        use std::net::{TcpListener, TcpStream};
        fn paramtest_tcp<R: rand::Rng>(mut rng: R) {
            let datalen = rng.gen_range(10..1024 * 512);
            let wopts_connector: Option<LimiterOptions> =
                get_random_options(&mut rng, datalen, 64 * 1024);
            let wopts_listener: Option<LimiterOptions> =
                get_random_options(&mut rng, datalen, 64 * 1024);
            let ropts_connector = get_random_options(&mut rng, datalen, 64 * 1024);
            let ropts_listener = get_random_options(&mut rng, datalen, 64 * 1024);

            // Step 1:     Connector write, listener reads
            let limiting_opts_1l = match (&wopts_connector, &ropts_listener) {
                (Some(wopts), Some(ropts)) => Some(wopts.intersect(ropts)),
                (None, Some(opts)) | (Some(opts), None) => Some(opts.clone()),
                (None, None) => None,
            };

            // Step 2:    Connector reads, listener writes
            let limiting_opts_2c = match (&ropts_connector, &wopts_listener) {
                (Some(ropts), Some(wopts)) => Some(wopts.intersect(ropts)),
                (None, Some(opts)) | (Some(opts), None) => Some(opts.clone()),
                (None, None) => None,
            };

            let thread_sync_l = Arc::new(Barrier::new(2));
            let thread_sync_c = thread_sync_l.clone();

            let data: Vec<u8> = (0..datalen).map(|_| rng.gen::<u8>()).collect();
            let data_c = data.clone();
            let datahash = get_data_hash(&data);
            let mut port = 10000 + rng.gen_range(0..(u16::MAX - 10000));

            let listener = loop {
                match TcpListener::bind(format!("127.0.0.1:{port}")) {
                    Ok(l) => break l,
                    Err(_) => {
                        port += 1;
                    }
                }
            };

            let connector = std::thread::spawn(move || {
                let stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
                thread_sync_c.wait();
                let mut limiter =
                    Limiter::new(stream, ropts_connector.clone(), wopts_connector.clone());

                let now = std::time::Instant::now();
                limiter.write_all(&data_c).unwrap();
                let elapsed = now.elapsed();
                assert_rate_limited("TWC", &wopts_connector, datalen, elapsed);

                thread_sync_c.wait();

                let mut buf = vec![0; datalen];
                let mut limiter = Limiter::new(
                    limiter.stream,
                    ropts_connector.clone(),
                    wopts_connector.clone(),
                );
                let now = std::time::Instant::now();
                limiter.read_exact(&mut buf).unwrap();
                let elapsed = now.elapsed();
                assert_rate_limited("TRC", &limiting_opts_2c, datalen, elapsed);
                assert_eq!(get_data_hash(&buf), datahash);
            });
            std::thread::sleep(Duration::from_millis(50));

            for stream in listener.incoming() {
                let mut buf = vec![0; datalen];
                thread_sync_l.wait();
                let mut limiter = Limiter::new(
                    stream.unwrap(),
                    ropts_listener.clone(),
                    wopts_listener.clone(),
                );

                let now = std::time::Instant::now();
                limiter.read_exact(&mut buf).unwrap();
                let elapsed = now.elapsed();
                assert_eq!(get_data_hash(&buf), datahash);
                assert_rate_limited("TRL", &limiting_opts_1l, datalen, elapsed);

                thread_sync_l.wait();
                println!("\nStarting step 2\n");

                let mut limiter =
                    Limiter::new(limiter.stream, ropts_listener, wopts_listener.clone());
                let now = std::time::Instant::now();
                limiter.write_all(&data).unwrap();
                let elapsed = now.elapsed();
                assert_rate_limited("TWL", &wopts_listener, datalen, elapsed);
                break;
            }
            assert!(connector.join().is_ok());
        }
        start_parametric_test(
            100,
            vec![
                15163516011027607440,
            ],
            paramtest_tcp,
        );
    }
}
