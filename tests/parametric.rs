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
    fn test_limit_speed() {
        fn paramtest_limit_speed<R: rand::Rng>(mut rng: R) {
            let datalen = rng.gen_range(10..1024*512);

            let outbuf = std::io::Cursor::new(vec![]);
            let wopts = get_random_options(&mut rng, datalen);
            let ropts = get_random_options(&mut rng, datalen);
            let mut limiter = Limiter::new(
                outbuf,
                ropts.clone(),
                wopts.clone(),
            );

            let data: Vec<u8> = (0..datalen).map(|_| rng.gen::<u8>()).collect();
            let buf = data.clone();
            let data_checksum = get_data_hash(&buf);
            
            println!("Writing {} data with opts {:?}", datalen, wopts);
            let now = std::time::Instant::now();
            let nwrite = limiter.write(&buf).unwrap();
            assert_eq!(nwrite, datalen);
            let elapsed = now.elapsed();
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

            let read_buf = limiter.stream;
            let mut limiter = Limiter::new(
                read_buf,
                ropts.clone(),
                wopts.clone(),
            );
            let mut buf = vec![0; datalen];
            println!("Reading {} data with opts {:?}", datalen, ropts);
            let now = std::time::Instant::now();
            let nread = limiter.read(buf.as_mut_slice()).unwrap();
            assert_eq!(nread, datalen);
            let elapsed = now.elapsed();
            if let Some(ref opts) = ropts {
                let rate = opts.window_length.min(opts.bucket_size as u128);
                if datalen as u128 > rate {
                    println!("{:?} > {:?}", elapsed, opts.window_time);
                    assert!(elapsed.as_nanos() > opts.window_time.as_nanos());
                } else {
                    println!("{:?} <= {:?}", elapsed, opts.window_time);
                    assert!(elapsed.as_nanos() <= opts.window_time.as_nanos());
                }
            } else {
                assert!(elapsed.as_nanos() < INSTANT_IO_EPS);
            }

            assert_eq!(get_data_hash(limiter.stream.get_ref()), data_checksum);
            assert_eq!(&data, limiter.stream.get_ref());
            println!("OK");
            println!("");
        }
        start_parametric_test(3_000_000, vec![
            14398057406427516238
        ], paramtest_limit_speed);
    }
}
