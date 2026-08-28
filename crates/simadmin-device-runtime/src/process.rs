use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub trait ProcessExecutor: Send + Sync {
    fn execute(
        &self,
        program: &Path,
        arguments: &[String],
        environment: &[(String, String)],
        timeout: Duration,
    ) -> Result<ProcessOutput, String>;
}

#[derive(Debug, Default)]
pub struct SystemProcessExecutor;

impl ProcessExecutor for SystemProcessExecutor {
    fn execute(
        &self,
        program: &Path,
        arguments: &[String],
        environment: &[(String, String)],
        timeout: Duration,
    ) -> Result<ProcessOutput, String> {
        let mut child = Command::new(program)
            .args(arguments)
            .envs(environment.iter().cloned())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("failed to start {}: {error}", program.display()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "failed to capture process stdout".to_owned())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "failed to capture process stderr".to_owned())?;
        let stdout_reader = thread::spawn(move || read_pipe(stdout));
        let stderr_reader = thread::spawn(move || read_pipe(stderr));
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("failed to poll process: {error}"))?
            {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "process timed out after {} ms",
                    timeout.as_millis()
                ));
            }
            thread::sleep(POLL_INTERVAL.min(timeout));
        };
        Ok(ProcessOutput {
            success: status.success(),
            exit_code: status.code(),
            stdout: join_pipe(stdout_reader, "stdout")?,
            stderr: join_pipe(stderr_reader, "stderr")?,
        })
    }
}

fn read_pipe(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_pipe(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    name: &str,
) -> Result<String, String> {
    let bytes = reader
        .join()
        .map_err(|_| format!("process {name} reader panicked"))?
        .map_err(|error| format!("failed to read process {name}: {error}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
