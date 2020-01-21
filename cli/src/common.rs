use crate::SharedOptions;

use probe_rs::{
    config::registry::{Registry, RegistryError, SelectionStrategy},
    cores::m0::FakeM0,
    coresight::access_ports::AccessPortError,
    flash::download::FileDownloadError,
    probe::{DebugProbeError, FakeProbe, Probe},
    session::Session,
    target::info::{self, ChipInfo},
};

use ron;

use std::error::Error;
use std::fmt;
use std::fs::File;
use std::path::Path;

#[derive(Debug)]
pub enum CliError {
    InfoReadError(info::ReadError),
    DebugProbe(DebugProbeError),
    AccessPort(AccessPortError),
    StdIO(std::io::Error),
    FileDownload(FileDownloadError),
    RegistryError(RegistryError),
    MissingArgument,
    UnableToOpenProbe(Option<&'static str>),
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        use CliError::*;

        match self {
            InfoReadError(e) => Some(e),
            DebugProbe(ref e) => Some(e),
            AccessPort(ref e) => Some(e),
            StdIO(ref e) => Some(e),
            RegistryError(ref e) => Some(e),
            MissingArgument => None,
            UnableToOpenProbe(_) => None,
            FileDownload(ref e) => Some(e),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use CliError::*;

        match self {
            InfoReadError(e) => e.fmt(f),
            DebugProbe(ref e) => e.fmt(f),
            AccessPort(ref e) => e.fmt(f),
            StdIO(ref e) => e.fmt(f),
            FileDownload(ref e) => e.fmt(f),
            RegistryError(ref e) => e.fmt(f),
            MissingArgument => write!(f, "Command expected more arguments."),
            UnableToOpenProbe(ref details) => match details {
                None => write!(f, "Unable to open probe."),
                Some(details) => write!(f, "Unable to open probe: {}", details),
            },
        }
    }
}

impl From<info::ReadError> for CliError {
    fn from(error: info::ReadError) -> Self {
        CliError::InfoReadError(error)
    }
}

impl From<AccessPortError> for CliError {
    fn from(error: AccessPortError) -> Self {
        CliError::AccessPort(error)
    }
}

impl From<DebugProbeError> for CliError {
    fn from(error: DebugProbeError) -> Self {
        CliError::DebugProbe(error)
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        CliError::StdIO(error)
    }
}

impl From<RegistryError> for CliError {
    fn from(error: RegistryError) -> Self {
        CliError::RegistryError(error)
    }
}

impl From<FileDownloadError> for CliError {
    fn from(error: FileDownloadError) -> Self {
        CliError::FileDownload(error)
    }
}

pub(crate) fn open_probe(index: Option<usize>) -> Result<Probe, CliError> {
    let available_probes = Probe::list_all();

    let device = match index {
        Some(index) => available_probes.get(index).ok_or(CliError::UnableToOpenProbe(Some("Unable to open the specified probe. Use the 'list' subcommand to see all available probes.")))?,
        None => {
            // open the default probe, if only one probe was found
            if available_probes.len() == 1 {
                &available_probes[0]
            } else {
                return Err(CliError::UnableToOpenProbe(Some("Multiple probes found. Please specify which probe to use using the -n parameter.")));
            }
        }
    };

    let probe = Probe::from_probe_info(&device)?;

    Ok(probe)
}

/// Takes a closure that is handed an `DAPLink` instance and then executed.
/// After the closure is done, the USB device is always closed,
/// even in an error case inside the closure!
pub(crate) fn with_device<F>(shared_options: &SharedOptions, f: F) -> Result<(), CliError>
where
    for<'a> F: FnOnce(Session) -> Result<(), CliError>,
{
    let mut probe = open_probe(shared_options.n)?;

    let strategy = if let Some(identifier) = &shared_options.target {
        SelectionStrategy::TargetIdentifier(identifier.into())
    } else {
        SelectionStrategy::ChipInfo(ChipInfo::read_from_rom_table(&mut probe)?)
    };

    let registry = Registry::from_builtin_families();

    let target = registry.get_target(strategy)?;

    let session = Session::new(target, probe);

    f(session)
}

pub(crate) fn with_dump<F>(shared_options: &SharedOptions, p: &Path, f: F) -> Result<(), CliError>
where
    for<'a> F: FnOnce(Session) -> Result<(), CliError>,
{
    let mut dump_file = File::open(p)?;

    let dump = ron::de::from_reader(&mut dump_file).unwrap();

    let core = FakeM0::new(dump);
    let fake_probe = FakeProbe::new();

    let probe = Probe::from_specific_probe(Box::new(fake_probe));

    let strategy = if let Some(identifier) = &shared_options.target {
        SelectionStrategy::TargetIdentifier(identifier.into())
    } else {
        unimplemented!();
    };

    let registry = Registry::from_builtin_families();

    let mut target = registry.get_target(strategy)?;

    target.core = Box::new(core);

    let session = Session::new(target, probe);

    f(session)
}
