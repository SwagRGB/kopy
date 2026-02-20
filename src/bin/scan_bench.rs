use kopy::scanner::{scan_directory, scan_directory_parallel};
use kopy::Config;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct BenchResult {
    sequential: Vec<Duration>,
    parallel: Vec<Duration>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let root = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("Usage: cargo run --bin scan_bench -- <root_path> [runs] [threads]");
            std::process::exit(2);
        }
    };

    let runs: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(5);
    let threads: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(8);

    let config = Config {
        source: root.clone(),
        destination: root.join("__bench_dest"),
        threads,
        ..Config::default()
    };

    println!(
        "Benchmarking scan on {}\nRuns: {} (threads={})",
        root.display(),
        runs,
        threads
    );

    // Warm up both scanners once to reduce first-run noise.
    let seq_tree = scan_directory(&root, &config, None)?;
    let par_tree = scan_directory_parallel(&root, &config, None)?;
    assert_parity(&seq_tree, &par_tree)?;

    let mut result = BenchResult {
        sequential: Vec::with_capacity(runs),
        parallel: Vec::with_capacity(runs),
    };

    for i in 0..runs {
        let seq_start = Instant::now();
        let seq_tree = scan_directory(&root, &config, None)?;
        let seq_elapsed = seq_start.elapsed();

        let par_start = Instant::now();
        let par_tree = scan_directory_parallel(&root, &config, None)?;
        let par_elapsed = par_start.elapsed();

        assert_parity(&seq_tree, &par_tree)?;

        result.sequential.push(seq_elapsed);
        result.parallel.push(par_elapsed);

        println!(
            "run {:>2}: seq={:>8.3} ms  par={:>8.3} ms",
            i + 1,
            seq_elapsed.as_secs_f64() * 1000.0,
            par_elapsed.as_secs_f64() * 1000.0
        );
    }

    let seq_avg = average_ms(&result.sequential);
    let par_avg = average_ms(&result.parallel);
    let speedup = if par_avg > 0.0 {
        seq_avg / par_avg
    } else {
        0.0
    };

    println!("\nSummary");
    println!("  sequential avg: {:>8.3} ms", seq_avg);
    println!("  parallel   avg: {:>8.3} ms", par_avg);
    println!("  speedup       : {:>8.2}x", speedup);

    Ok(())
}

fn average_ms(values: &[Duration]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let sum_ms: f64 = values.iter().map(|d| d.as_secs_f64() * 1000.0).sum();
    sum_ms / values.len() as f64
}

fn assert_parity(seq: &kopy::FileTree, par: &kopy::FileTree) -> Result<(), String> {
    if seq.total_files != par.total_files {
        return Err(format!(
            "File count mismatch: sequential={} parallel={}",
            seq.total_files, par.total_files
        ));
    }
    if seq.total_size != par.total_size {
        return Err(format!(
            "Total size mismatch: sequential={} parallel={}",
            seq.total_size, par.total_size
        ));
    }
    Ok(())
}
