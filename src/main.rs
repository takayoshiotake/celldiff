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
    /// Original UTF-8 CSV (first record is included)
    before: PathBuf,
    /// Updated UTF-8 CSV
    after: PathBuf,
    /// Output XLSX report
    #[arg(short, long, value_name = "FILE")]
    output: PathBuf,
    /// Replace an existing report (input files are always protected)
    #[arg(long)]
    force: bool,
}

fn run(args: Args) -> Result<bool, AppError> {
    let mut before = CsvInput::open(&args.before)?;
    let mut after = CsvInput::open(&args.after)?;
    validate_paths(&args.before, &args.after, &args.output, args.force)?;
    let mut report = Report::new()?;
    let summary = compare_all(&mut before, &mut after, |d| report.write(d))?;
    report.save(&args.output, args.force, summary, &args.before, &args.after)?;
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
