use converter::{decode, encode};
use std::{
    fs,
    io::{self, Write},
    path::Path,
    process::ExitCode,
};

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;

    if let Err(e) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(e);
    }

    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();

    let [op, input, output] = args.as_slice() else {
        return Err("usage: converter encode|decode INPUT OUTPUT (never overwrites)".into());
    };

    let bytes = fs::read(input)?;

    match op.to_str() {
        Some("encode") => write_new(Path::new(output), &encode(&bytes)?)?,
        Some("decode") => write_new(Path::new(output), decode(&bytes)?)?,
        _ => return Err("expected encode or decode".into()),
    }

    Ok(())
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
