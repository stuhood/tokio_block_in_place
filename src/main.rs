use std::io::{self, Write};
use std::os::unix::io::{FromRawFd, IntoRawFd};

use futures::{stream, StreamExt};
use os_pipe::PipeWriter;
use tokio::fs::File;
use tokio::runtime::Builder;
use tokio_util::io::ReaderStream;

fn make_pipe() -> io::Result<(ReaderStream<File>, PipeWriter)> {
    let (pipe_reader, pipe_writer) = os_pipe::pipe()?;
    let read_handle =
        File::from_std(unsafe { std::fs::File::from_raw_fd(pipe_reader.into_raw_fd()) });
    Ok((ReaderStream::new(read_handle), pipe_writer))
}

fn main() {
    let n = 4;

    // Create a runtime with N core threads and effectively unlimited blocking threads.
    let runtime = Builder::new_multi_thread()
        .worker_threads(n)
        .max_blocking_threads(n * 1024)
        .enable_all()
        .build()
        .unwrap();

    // Create two pipes, both of which will be read from, but only one of which will be written to.
    let (stream1, writer1) = make_pipe().unwrap();
    let (stream2, _writer2) = make_pipe().unwrap();

    // Spawn N tasks writing to one of the pipes.
    for _ in 0..n {
        let mut writer1 = writer1.try_clone().unwrap();
        let _ = runtime.spawn(async move {
            loop {
                // NB: The `block_in_place` here prevents a deadlock caused by the sender tasks
                // blocking all core threads.
                tokio::task::block_in_place(|| {
                    writer1.write(b"something.").unwrap();
                })
            }
        });
    }

    // Spawn a single task that will poll both pipes (using AsyncRead via ReaderStream).
    let task = runtime.spawn(async move {
        let mut idx: usize = 0;
        let mut output_stream = stream::select(stream1, stream2);
        while let Some(_output) = output_stream.next().await {
            println!("Read {idx}th output.");
            idx += 1;
        }
    });

    // Block on that task. Note that we spawn and then block_on so that the reader task is
    // definitely running on the core threads.
    runtime.block_on(task).unwrap();
}
