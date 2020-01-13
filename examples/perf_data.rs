#![type_length_limit="9479963"]
use clap::*;
use s3_upload::*;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use futures::{
    future::ok
};

macro_rules! all_file_paths {
    ($dir_path:expr $(, max_open = $max_open:expr)?) => {
        walkdir::WalkDir::new(&$dir_path)
            $(.max_open($max_open))?
            .into_iter()
            .filter_map(|entry|
                entry.ok().and_then(|entry| {
                    if entry.file_type().is_file() {
                        Some(entry.path().to_owned())
                    } else {
                        None
                    }
                }))};
}

fn main() {
    let mut app = App::new("Example 'perf_data'")
        .before_help("Upload a directory to S3 on localhost.")
        .arg(
            Arg::with_name("source")
                .help("Path to a folder to upload to S3")
                .required(true),
        )
        .arg(
            Arg::with_name("dest_bucket")
                .help("Destination bucket")
                .required(true),
        )
        .arg(
            Arg::with_name("dest_prefix")
                .help("Destination prefix")
                .required(true),
        )
        .arg(
            Arg::with_name("parallelization")
                .short("n")
                .takes_value(true)
                .help("Maximum number of simultaneous upload requests"),
        );
    let matches = app.clone().get_matches();

    if let (Some(path), Some(bucket), Some(prefix)) = (
        matches.value_of("source"),
        matches.value_of("dest_bucket"),
        matches.value_of("dest_prefix"),
    ) {
        let parallelization = value_t_or_exit!(matches.value_of("parallelization"), usize);
        benchmark_s3_upload(
            Path::new(path).to_path_buf(),
            bucket.to_owned(),
            prefix.to_owned(),
            parallelization,
        );
        println!("Done");
    } else {
        app.print_help().unwrap()
    }
}

fn benchmark_s3_upload(
    dir_path: PathBuf,
    bucket: String,
    prefix: String,
    copy_parallelization: usize,
) {
    let s3 = s3_upload::testing_s3_client();

    let cfg = UploadConfig {
        copy_parallelization,
        ..Default::default()
    };

    upload_perf_log_init(&mut std::io::stdout());
    let progress = |res: UploadFileResult| {
        upload_perf_log_update(&mut std::io::stdout(), res);
        ok(())
    };
    let files_to_upload = all_file_paths!(dir_path);
    let future = s3_upload_files(
        s3,
        bucket,
        files_to_upload,
        move |path| PathBuf::from(&prefix).join(path.strip_prefix(&dir_path).unwrap()),
        cfg,
        progress,
        Default::default,
    );
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(future).unwrap();
}

// Helpers for writing data
macro_rules! write_cell {
    ($out:expr, $x:expr) => {
        let _ = write!($out, "{0: >18}", format!("{:.5}", $x));
    };
}
pub fn upload_perf_log_init<W: Write>(out: &mut W) {
    let _ = writeln!(
        out,
        "{0: >w$}{1: >w$}{2: >w$}{3: >w$}{4: >w$}{5: >w$}",
        "attempts",
        "bytes",
        "success_ms",
        "total_ms",
        "MBps",
        "MBps est",
        w = 18
    );
}
pub fn upload_perf_log_update<W: Write>(out: &mut W, res: UploadFileResult) {
    // TODO: Write performance data to file with tokio
    let megabytes = res.bytes as f64 / 1_000_000.0;
    let speed = megabytes / res.success_time.as_secs_f64();
    write_cell!(out, res.attempts);
    write_cell!(out, res.bytes);
    write_cell!(out, res.success_time.as_millis());
    write_cell!(out, res.total_time.as_millis());
    write_cell!(out, speed);
    write_cell!(out, res.est);
    let _ = writeln!(out);
}
