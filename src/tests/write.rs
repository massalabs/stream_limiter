use std::{io::Write, time::Duration};

use super::utils::assert_checksum_samedata;
use crate::{Limiter, LimiterOptions};

#[test]
fn one_byte_each_second() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limiter = Limiter::new(
        outbuf,
        None,
        Some(LimiterOptions::new(1, Duration::from_secs(1), 10)),
    );
    assert!(limiter.limits().1);
    let now = std::time::Instant::now();
    let buf = [42u8; 10];
    limiter.write(&buf).unwrap();
    assert_eq!(now.elapsed().as_secs(), 10);
    assert_checksum_samedata::<10>(&limiter.stream.into_inner(), 42);
}

#[test]
fn one_byte_each_two_hundreds_fifty_millis() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limiter = Limiter::new(
        outbuf,
        None,
        Some(LimiterOptions::new(1, Duration::from_millis(250), 10)),
    );
    assert!(limiter.limits().1);
    let now = std::time::Instant::now();
    let buf = [21u8; 10];
    limiter.write(&buf).unwrap();
    assert_eq!(now.elapsed().as_secs(), 2);
    assert_checksum_samedata::<10>(&limiter.stream.into_inner(), 21);
}

#[test]
fn two_byte_each_second() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limiter = Limiter::new(
        outbuf,
        None,
        Some(LimiterOptions::new(2, Duration::from_secs(1), 10)),
    );
    assert!(limiter.limits().1);
    let now = std::time::Instant::now();
    let buf = [18u8; 10];
    limiter.write(&buf).unwrap();
    assert_eq!(now.elapsed().as_secs(), 5);
    assert_checksum_samedata::<10>(&limiter.stream.into_inner(), 18);
}

#[test]
fn write_instant() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limiter = Limiter::new(outbuf, None, None);
    assert!(!limiter.limits().1);
    let now = std::time::Instant::now();
    let buf = [33u8; 10];
    limiter.write(&buf).unwrap();
    assert_eq!(now.elapsed().as_secs(), 0);
    assert_checksum_samedata::<10>(&limiter.stream.into_inner(), 33);
}

#[test]
fn test_burst() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limiter = Limiter::new(
        outbuf,
        None,
        Some(LimiterOptions::new(1, Duration::from_secs(1), 10)),
    );
    assert!(limiter.limits().1);
    std::thread::sleep(Duration::from_secs(10));

    // Write a second byte of 10 bytes. Should be instant because we waited above
    let now = std::time::Instant::now();
    let buf = [12u8; 10];
    limiter.write(&buf).unwrap();
    assert_eq!(now.elapsed().as_secs(), 0);
    assert_checksum_samedata::<10>(&limiter.stream.into_inner(), 12);
}

#[test]
fn oneko_limit() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limiter = Limiter::new(
        outbuf,
        None,
        Some(LimiterOptions::new(1024, Duration::from_secs(1), 12 * 1024)),
    );
    assert!(limiter.limits().1);
    let now = std::time::Instant::now();
    let buf = [88u8; 11 * 1024];
    limiter.write(&buf).unwrap();
    assert_eq!(now.elapsed().as_secs(), 11, "{:?}", now.elapsed());
    assert_checksum_samedata::<11264>(&limiter.stream.into_inner(), 88);
}

#[test]
fn splitted_write() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limiter = Limiter::new(
        outbuf,
        None,
        Some(LimiterOptions::new(10, Duration::from_secs(1) / 1024, 12)),
    );
    assert!(limiter.limits().1);

    let now = std::time::Instant::now();
    let buf = [66u8; 8];
    limiter.write(&buf).unwrap();

    let buf = [66u8; (11 * 1024) - 8];
    limiter.write(&buf).unwrap();

    assert_eq!(now.elapsed().as_secs(), 1, "{:?}", now.elapsed());
    assert_checksum_samedata::<11264>(&limiter.stream.into_inner(), 66);
}

#[test]
fn write_bucket_full() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limiter = Limiter::new(
        outbuf,
        None,
        Some(LimiterOptions::new(100, Duration::from_secs(1), 10)),
    );
    assert!(limiter.limits().1);

    // 100 bytes with write peak
    let buf = [128u8; 100];
    limiter.write(&buf).unwrap();

    std::thread::sleep(Duration::from_secs(1));

    let now = std::time::Instant::now();
    // 10 bytes from bucket + 100 bytes / sec -> 1s to write 110 bytes
    let mut buf = [128u8; 110];
    limiter.write(&mut buf).unwrap();

    assert_eq!(now.elapsed().as_secs(), 1, "{:?}", now.elapsed());
    assert_checksum_samedata::<210>(&limiter.stream.into_inner(), 128);
}

#[test]
fn test_max_limit() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limiter = Limiter::new(
        outbuf,
        None,
        Some(LimiterOptions::new(u64::MAX, Duration::ZERO, u64::MAX)),
    );
    assert!(limiter.limits().1);

    let buf = [144u8; 100];
    limiter.write(&buf).unwrap();

    assert_checksum_samedata::<100>(&limiter.stream.into_inner(), 144);
}

#[test]
fn write_timeout() {
    let outbuf = std::io::Cursor::new(vec![]);
    let mut limopt = LimiterOptions::new(1, Duration::from_secs(1), 10);
    limopt.set_timeout(Duration::from_secs(1));
    let mut limiter = Limiter::new(outbuf, None, Some(limopt));
    assert!(limiter.limits().1);

    let mut buf = [128u8; 110];
    let now = std::time::Instant::now();
    let res = limiter.write(&mut buf);
    assert_eq!(now.elapsed().as_millis(), 1000);
    assert!(res.is_err());
}
