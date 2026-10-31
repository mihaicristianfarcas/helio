//! How long the real Mainline DHT takes: join, publish, resolve from a second
//! client (today's slot and yesterday's, at once, as Helio looks), and four
//! records published together, as a host with four paired clients does.
//! Publishes five sealed records under throwaway secrets, and keeps its
//! sequence number in a temporary folder.
//!
//!   cargo run -p helio-nat --example dht
use std::time::{Duration, Instant};

use helio_nat::keys::Sequence;
use helio_nat::rendezvous::{random_32, Kind, Record, Rendezvous};

fn wait_ready(r: &Rendezvous, what: &str, t: Instant) {
    while !r.is_ready() {
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("{what} joined in {:?}", t.elapsed());
}

fn main() {
    let dir = std::env::temp_dir().join(format!("helio-dht-example-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let seq = Sequence::open(dir.join("rendezvous.toml"), |p, b| std::fs::write(p, b));
    let secret = random_32();
    let t = Instant::now();
    let a = Rendezvous::join().unwrap();
    let b = Rendezvous::join().unwrap();
    wait_ready(&a, "publisher", t);
    wait_ready(&b, "resolver", t);

    let endpoints = vec!["203.0.113.7:47800".parse().unwrap()];
    let record = Record::new(Kind::Host, endpoints.clone());
    let t = Instant::now();
    a.publish(&secret, &record, &seq).unwrap();
    println!("published in {:?}", t.elapsed());

    for i in 0..3 {
        let t = Instant::now();
        let got = b.resolve(&secret, Kind::Host);
        println!(
            "resolve #{i}: {:?} in {:?}",
            got.as_ref().map(|r| &r.endpoints),
            t.elapsed()
        );
    }

    let secrets: Vec<_> = (0..4).map(|_| random_32()).collect();
    let t = Instant::now();
    std::thread::scope(|s| {
        for secret in &secrets {
            let (a, seq, endpoints) = (&a, &seq, endpoints.clone());
            s.spawn(move || a.publish(secret, &Record::new(Kind::Host, endpoints), seq));
        }
    });
    println!("4 published together in {:?}", t.elapsed());
    let _ = std::fs::remove_dir_all(dir);
}
