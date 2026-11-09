use std::error::Error;
use std::io::{self, Read, Write};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{
    ChildKiller, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system,
};

const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy)]
pub enum Streams<'a> {
    Terminal,
    InputRedirected,
    OutputRedirected(&'a std::path::Path),
}

enum Event {
    Output(Vec<u8>),
    Closed,
    Exited(io::Result<ExitStatus>),
}

pub struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    events: Receiver<Event>,
    reader: Option<JoinHandle<()>>,
    waiter: Option<JoinHandle<()>>,
    parser: vt100::Parser,
    transcript: Vec<u8>,
    status: Option<ExitStatus>,
    closed: bool,
}

impl Session {
    pub fn spawn(command: &Command, streams: Streams<'_>) -> Result<Self, Box<dyn Error>> {
        let mut cmd = match streams {
            Streams::Terminal => CommandBuilder::new(command.get_program()),
            Streams::InputRedirected => {
                let mut cmd = CommandBuilder::new("/bin/sh");
                cmd.args(["-c", "exec \"$@\" </dev/null", "terminal-test"]);
                cmd.arg(command.get_program());
                cmd
            }
            Streams::OutputRedirected(path) => {
                let mut cmd = CommandBuilder::new("/bin/sh");
                cmd.args([
                    "-c",
                    "exec \"$@\" >\"$TERMINAL_TEST_OUTPUT\"",
                    "terminal-test",
                ]);
                cmd.arg(command.get_program());
                cmd.env("TERMINAL_TEST_OUTPUT", path);
                cmd
            }
        };
        cmd.args(command.get_args());
        if let Some(directory) = command.get_current_dir() {
            cmd.cwd(directory);
        }
        for (name, value) in command.get_envs() {
            if let Some(value) = value {
                cmd.env(name, value);
            } else {
                cmd.env_remove(name);
            }
        }
        cmd.env("TERM", "xterm-256color");
        let pair = native_pty_system().openpty(size(40, 120))?;
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let mut child = pair.slave.spawn_command(cmd)?;
        let killer = child.clone_killer();
        drop(pair.slave);
        let (sender, events) = mpsc::channel();
        let output_sender = sender.clone();
        let reader = thread::spawn(move || {
            let mut buffer = [0; 8192];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0
                    || output_sender
                        .send(Event::Output(buffer[..count].to_vec()))
                        .is_err()
                {
                    break;
                }
            }
            let _ = output_sender.send(Event::Closed);
        });
        let waiter = thread::spawn(move || {
            let _ = sender.send(Event::Exited(child.wait()));
        });
        Ok(Self {
            master: pair.master,
            writer,
            killer,
            events,
            reader: Some(reader),
            waiter: Some(waiter),
            parser: vt100::Parser::new(40, 120, 0),
            transcript: Vec::new(),
            status: None,
            closed: false,
        })
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub fn contents(&self) -> String {
        self.screen().contents()
    }

    pub fn transcript(&self) -> &[u8] {
        &self.transcript
    }

    pub fn send(&mut self, input: &[u8]) -> io::Result<()> {
        self.writer.write_all(input)?;
        self.writer.flush()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<(), Box<dyn Error>> {
        self.master.resize(size(rows, cols))?;
        self.parser.screen_mut().set_size(rows, cols);
        Ok(())
    }

    pub fn wait_for(&mut self, predicate: impl Fn(&vt100::Screen) -> bool) -> io::Result<()> {
        self.wait_for_within(TIMEOUT, predicate)
    }

    pub fn wait_for_within(
        &mut self,
        timeout: Duration,
        predicate: impl Fn(&vt100::Screen) -> bool,
    ) -> io::Result<()> {
        let deadline = Instant::now() + timeout;
        while !predicate(self.screen()) {
            if self.closed {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    self.contents(),
                ));
            }
            self.receive(deadline, true)?;
        }
        Ok(())
    }

    pub fn finish(&mut self) -> io::Result<ExitStatus> {
        self.complete(true)
    }

    pub fn terminate(&mut self) -> io::Result<ExitStatus> {
        let _ = self.killer.kill();
        self.complete(false)
    }

    fn complete(&mut self, capture_output: bool) -> io::Result<ExitStatus> {
        let deadline = Instant::now() + TIMEOUT;
        while !self.closed || self.status.is_none() {
            self.receive(deadline, capture_output)?;
        }
        self.join_threads();
        self.status
            .clone()
            .ok_or_else(|| io::Error::other("child exit status missing"))
    }

    fn timeout_error(&self) -> io::Error {
        io::Error::new(
            io::ErrorKind::TimedOut,
            format!(
                "terminal deadline expired; status={:?}, output_closed={}, captured_bytes={}\n{}",
                self.status,
                self.closed,
                self.transcript.len(),
                self.contents()
            ),
        )
    }

    fn receive(&mut self, deadline: Instant, capture_output: bool) -> io::Result<()> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| self.timeout_error())?;
        let event = self
            .events
            .recv_timeout(remaining)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => self.timeout_error(),
                mpsc::RecvTimeoutError::Disconnected => {
                    io::Error::new(io::ErrorKind::BrokenPipe, error)
                }
            })?;
        match event {
            Event::Output(bytes) if capture_output => {
                self.parser.process(&bytes);
                self.transcript.extend(bytes);
            }
            Event::Output(_) => {}
            Event::Closed => self.closed = true,
            Event::Exited(status) => self.status = Some(status?),
        }
        Ok(())
    }

    fn join_threads(&mut self) {
        if self.closed
            && let Some(reader) = self.reader.take()
        {
            let _ = reader.join();
        }
        if self.status.is_some()
            && let Some(waiter) = self.waiter.take()
        {
            let _ = waiter.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.status.is_none() {
            let _ = self.terminate();
        }
        self.join_threads();
    }
}

fn size(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}
