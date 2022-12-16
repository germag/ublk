// SPDX-License-Identifier: MIT

use std::io;
use thiserror::Error;
#[derive(Error, Debug)]
pub enum Error {
    #[error("io_uring full submission queue")]
    FullSubmissionQueue(#[from] io_uring::squeue::PushError),

    #[error("Io: {source}")]
    Io {
        #[from]
        source: io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
