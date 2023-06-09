use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

#[allow(dead_code)]
pub fn start_parametric_test<F>(nbiter: usize, regressions: Vec<u64>, function: F)
where
    F: Fn(SmallRng),
{
    for seed in regressions.iter() {
        println!("Test regression seed {}", seed);
        function(SmallRng::seed_from_u64(*seed));
    }
    let mut seeder = SmallRng::from_entropy();

    let nspace = nbiter.to_string().len();
    for n in 0..nbiter {
        let new_seed: u64 = seeder.gen();
        print!("{:1$}", n+1, nspace);
        println!("/{}| Seed {:20}", nbiter, new_seed);
        function(SmallRng::seed_from_u64(new_seed));
        println!("\tOK");
    }
}

