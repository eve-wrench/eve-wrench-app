// Logs go to stderr and to a file in the data directory, so a launch from a
// desktop shortcut (or a Windows build, which has no console) still leaves
// something to attach to a bug report. GPUI reports platform and renderer
// failures through `log` as well.

use std::backtrace::{Backtrace, BacktraceStatus};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

use eve_wrench_core::Locations;
use eve_wrench_core::updates::APP_VERSION;

pub fn file_path() -> PathBuf {
    Locations::default_data_dir().join("eve-wrench.log")
}

pub fn init() {
    let path = file_path();
    // Each launch starts a fresh file; a failure here only costs the file
    let file = path
        .parent()
        .and_then(|dir| fs::create_dir_all(dir).ok())
        .and_then(|_| File::create(&path).ok());

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,eve_wrench=info,eve_wrench_core=info"),
    )
    .target(env_logger::Target::Pipe(Box::new(Tee(file))))
    .init();

    // Panics go through the log so they land in the file too
    std::panic::set_hook(Box::new(|info| {
        let backtrace = Backtrace::capture();
        if backtrace.status() == BacktraceStatus::Captured {
            log::error!("{info}\n{backtrace}");
        } else {
            log::error!("{info}");
        }
    }));

    log::info!(
        "EVE Wrench {APP_VERSION} on {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    log::info!("Logging to {}", path.display());
}

struct Tee(Option<File>);

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // stderr may be closed when launched without a terminal
        let _ = io::stderr().write_all(buf);
        if let Some(file) = &mut self.0 {
            file.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stderr().flush();
        match &mut self.0 {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}
