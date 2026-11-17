//! Concurrent HTTP load generator used by `benchmarks/run.ps1`.
//!
//! Keeps `concurrency` connections busy for `seconds` and reports throughput
//! plus p50/p95/p99 latency, so both servers are measured identically.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let url = Arc::new(
        args.next()
            .unwrap_or_else(|| "http://127.0.0.1:8080/user/1".to_string()),
    );
    let seconds: u64 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10);
    let concurrency: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(50);

    let client = Arc::new(
        reqwest::Client::builder()
            .pool_max_idle_per_host(concurrency)
            .build()
            .expect("build http client"),
    );

    let stop = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(std::sync::Mutex::new(Vec::<f64>::new()));

    let mut workers = Vec::new();
    for _ in 0..concurrency {
        let client = Arc::clone(&client);
        let url = Arc::clone(&url);
        let stop = Arc::clone(&stop);
        let completed = Arc::clone(&completed);
        let failed = Arc::clone(&failed);
        let latencies = Arc::clone(&latencies);
        workers.push(tokio::spawn(async move {
            while !stop.load(Ordering::Relaxed) {
                let started = Instant::now();
                match client.get(url.as_str()).send().await {
                    Ok(response) if response.status().is_success() => {
                        latencies.lock().unwrap().push(started.elapsed().as_secs_f64() * 1000.0);
                        completed.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    let started = Instant::now();
    tokio::time::sleep(Duration::from_secs(seconds)).await;
    stop.store(true, Ordering::Relaxed);
    for worker in workers {
        let _ = worker.await;
    }
    let elapsed = started.elapsed().as_secs_f64();

    let samples = {
        let mut samples = latencies.lock().unwrap().clone();
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        samples
    };
    let count = samples.len();
    let percentile = |fraction: f64| -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        let index = ((samples.len() as f64) * fraction) as usize;
        samples[index.min(samples.len() - 1)]
    };

    println!("requests   : {count}");
    println!("failed     : {}", failed.load(Ordering::Relaxed));
    println!("duration   : {elapsed:.2} s");
    println!("throughput : {:.0} req/s", count as f64 / elapsed);
    println!("p50        : {:.2} ms", percentile(0.50));
    println!("p95        : {:.2} ms", percentile(0.95));
    println!("p99        : {:.2} ms", percentile(0.99));
}