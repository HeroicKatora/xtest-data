use std::error::Error;
use std::io;
use std::process::{Command, Output, Stdio};

#[derive(Debug)]
#[allow(dead_code)]
pub struct LocatedError {
    location: &'static std::panic::Location<'static>,
    inner: io::Error,
}

pub trait GoodOutput {
    fn success(&mut self) -> Result<(), io::Error>;
    fn output(&mut self) -> Result<Output, io::Error>;
    fn input_output(&mut self, inp: &dyn AsRef<[u8]>) -> Result<Output, io::Error>;
}

pub trait ParseOutput {
    fn into_string(self) -> Result<String, io::Error>;
}

impl GoodOutput for Command {
    fn success(&mut self) -> Result<(), io::Error> {
        let status = self.status()?;
        if !status.success() {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(())
    }

    fn output(&mut self) -> Result<Output, io::Error> {
        self.stdout(Stdio::piped());
        let output = self.output()?;
        if !output.status.success() {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(output)
    }

    fn input_output(&mut self, inp: &dyn AsRef<[u8]>) -> Result<Output, io::Error> {
        self.stdin(Stdio::piped());
        self.stdout(Stdio::piped());
        let mut child = self.spawn()?;
        let output = {
            let mut stdin = child.stdin.take().unwrap();
            std::io::Write::write(&mut stdin, inp.as_ref())?;
            // Terminate the input here.
            drop(stdin);
            child.wait_with_output()?
        };
        if !output.status.success() {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(output)
    }
}

impl ParseOutput for Output {
    fn into_string(self) -> Result<String, io::Error> {
        String::from_utf8(self.stdout).map_err(as_io_error)
    }
}

/// Create an IO error, with its message just point to the source.
#[track_caller]
pub fn undiagnosed_io_error() -> impl FnMut() -> io::Error {
    let location = std::panic::Location::caller();
    move || io::Error::new(io::ErrorKind::Other, location.to_string())
}

/// Rewrap an error as IO error because we're lazy and this is a decent enough error type.
pub fn as_io_error<T>(err: T) -> io::Error
where
    T: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    io::Error::new(io::ErrorKind::Other, err)
}

/// Wrap the errors in such a way that we can figure out where they came from.
/// It's kind of amazing that this is stable o_o
#[track_caller]
pub fn anchor_error<E: Error + Send + Sync + 'static>() -> impl FnMut(E) -> LocatedError {
    let location = std::panic::Location::caller();
    move |inner| if <dyn core::any::Any>::is::<io::Error>(&inner) {
        LocatedError {
            location,
            inner: *Box::<dyn core::any::Any>::downcast::<io::Error>(Box::new(inner)).unwrap(),
        }
    } else {
        LocatedError {
            location,
            inner: std::io::Error::new(std::io::ErrorKind::Other, inner),
        }
    }
}
