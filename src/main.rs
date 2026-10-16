use celldiff::{
    AppError,
    compare::compare_all,
    input::CsvInput,
    output::{Report, validate_paths},
};
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Left UTF-8 CSV, used as the comparison baseline (first record is included)
    left: PathBuf,
    /// Right UTF-8 CSV, compared against the left input
    right: PathBuf,
    /// Output XLSX report
    #[arg(short, long, value_name = "FILE")]
    output: PathBuf,
    /// Replace an existing report (input files are always protected)
    #[arg(long)]
    force: bool,
}

fn run(args: Args) -> Result<bool, AppError> {
    let mut left = CsvInput::open(&args.left)?;
    let mut right = CsvInput::open(&args.right)?;
    validate_paths(&args.left, &args.right, &args.output, args.force)?;
    let mut report = Report::new()?;
    let summary = compare_all(&mut left, &mut right, |d| report.write(d))?;
    report.save(&args.output, args.force, summary, &args.left, &args.right)?;
    println!(
        "changed_cells={} changed_records={} modified={} added={} removed={} output={}",
        summary.changed_cells,
        summary.changed_records,
        summary.modified,
        summary.added,
        summary.removed,
        args.output.display()
    );
    Ok(summary.changed_cells != 0)
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(1),
        Err(error) => {
            eprintln!("celldiff: {error}");
            ExitCode::from(2)
        }
    }
}
