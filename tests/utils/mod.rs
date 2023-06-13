use hex_literal::hex;
use sha2::Digest;

pub mod paramtests;

// The checksum and the size of the data (to trim the buffer)
pub const FILE_BIG: ([u8; 32], usize) = (
    hex!("55e28ecbd9ea1df018ffacd137ee8d62551eb2d6fbd46508bca7809005ff267a"),
    11264,
);
pub const FILE_LITTLE: ([u8; 32], usize) = (
    hex!("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"),
    4,
);
pub const FILE_TEST: ([u8; 32], usize) = (
    hex!("f29bc64a9d3732b4b9035125fdb3285f5b6455778edca72414671e0ca3b2e0de"),
    20,
);

pub fn assert_checksum(buf: &[u8], file: &([u8; 32], usize)) {
    let mut hasher = sha2::Sha256::new();
    hasher.update(&buf[..file.1]);
    assert_eq!(hasher.finalize()[..], file.0[..]);
}

pub fn assert_checksum_samedata<const N: usize>(buf: &[u8], data: u8) {
    let mut hasher = sha2::Sha256::new();
    let samedata = [data; N];
    if N <= 50 {
        println!("{:?}\n{:?}", samedata, buf);
    }
    hasher.update([data; N]);
    let samedata_hash = hasher.finalize();

    let mut hasher = sha2::Sha256::new();
    hasher.update(buf);
    assert_eq!(hasher.finalize()[..], samedata_hash[..]);
}
