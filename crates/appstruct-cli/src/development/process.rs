use crate::environment::ProjectEnvironment;
use std::io::{self, BufRead, BufReader, Read};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

pub(super) struct DevProcesses {
    api: Child,
    web: Child,
    logs: Vec<thread::JoinHandle<()>>,
}

impl DevProcesses {
    pub(super) fn spawn(
        project: &Path,
        environment: &ProjectEnvironment,
        database_url: &str,
        api_port: u16,
        web_port: u16,
    ) -> io::Result<Self> {
        let api_url = format!("http://127.0.0.1:{api_port}");
        let web_url = format!("http://127.0.0.1:{web_port}");
        let api = start_api(project, environment, database_url, api_port, &web_url)?;
        let web = match start_web(project, environment, web_port, &api_url) {
            Ok(web) => web,
            Err(error) => {
                let mut api = api;
                terminate(&mut api);
                return Err(error);
            }
        };
        let mut api = api;
        let mut web = web;
        let logs = vec![
            log_pipe("api", api.stdout.take()),
            log_pipe("api", api.stderr.take()),
            log_pipe("web", web.stdout.take()),
            log_pipe("web", web.stderr.take()),
        ]
        .into_iter()
        .flatten()
        .collect();
        Ok(Self { api, web, logs })
    }

    pub(super) fn restart_api(
        &mut self,
        project: &Path,
        environment: &ProjectEnvironment,
        database_url: &str,
        api_port: u16,
        web_port: u16,
    ) -> io::Result<()> {
        terminate(&mut self.api);
        let mut api = start_api(
            project,
            environment,
            database_url,
            api_port,
            &format!("http://127.0.0.1:{web_port}"),
        )?;
        self.logs.extend(
            [
                log_pipe("api", api.stdout.take()),
                log_pipe("api", api.stderr.take()),
            ]
            .into_iter()
            .flatten(),
        );
        self.api = api;
        Ok(())
    }

    pub(super) fn restart(
        &mut self,
        project: &Path,
        environment: &ProjectEnvironment,
        database_url: &str,
        api_port: u16,
        web_port: u16,
    ) -> io::Result<()> {
        self.stop();
        *self = Self::spawn(project, environment, database_url, api_port, web_port)?;
        Ok(())
    }

    pub(super) fn failure(&mut self) -> io::Result<Option<String>> {
        if let Some(status) = self.api.try_wait()? {
            return Ok(Some(format!("API exited with {status}")));
        }
        if let Some(status) = self.web.try_wait()? {
            return Ok(Some(format!("web server exited with {status}")));
        }
        Ok(None)
    }

    pub(super) fn stop(&mut self) {
        terminate(&mut self.web);
        terminate(&mut self.api);
        for handle in self.logs.drain(..) {
            let _ = handle.join();
        }
    }
}

fn start_api(
    project: &Path,
    environment: &ProjectEnvironment,
    database_url: &str,
    api_port: u16,
    web_url: &str,
) -> io::Result<Child> {
    let mut command = Command::new(
        project
            .join(".appstruct/cache/backend-target/debug")
            .join(crate::build::backend_binary_name(project)?),
    );
    command
        .current_dir(project)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    environment.apply(&mut command);
    command
        .env("DATABASE_URL", database_url)
        .env("APPSTRUCT_BIND", format!("127.0.0.1:{api_port}"))
        .env(
            "APPSTRUCT_ALLOWED_ORIGIN",
            environment
                .get("APPSTRUCT_ALLOWED_ORIGIN")
                .unwrap_or_else(|| web_url.to_owned()),
        )
        .env(
            "APPSTRUCT_FRONTEND_URL",
            environment
                .get("APPSTRUCT_FRONTEND_URL")
                .unwrap_or_else(|| web_url.to_owned()),
        );
    isolate_process_group(&mut command);
    command.spawn()
}

fn start_web(
    project: &Path,
    environment: &ProjectEnvironment,
    web_port: u16,
    api_url: &str,
) -> io::Result<Child> {
    let mut command = Command::new("pnpm");
    command
        .current_dir(project.join("generated/web"))
        .args([
            "run",
            "dev",
            "--host",
            "127.0.0.1",
            "--port",
            &web_port.to_string(),
            "--strictPort",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    environment.apply(&mut command);
    command.env(
        "VITE_API_URL",
        environment
            .get("VITE_API_URL")
            .unwrap_or_else(|| api_url.to_owned()),
    );
    isolate_process_group(&mut command);
    command.spawn()
}

impl Drop for DevProcesses {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) struct ManagedDatabase {
    project: Option<std::path::PathBuf>,
    environment: Option<ProjectEnvironment>,
    started: bool,
}

impl ManagedDatabase {
    pub(super) fn external() -> Self {
        Self {
            project: None,
            environment: None,
            started: false,
        }
    }

    pub(super) fn start(project: &Path, environment: &ProjectEnvironment) -> io::Result<Self> {
        let mut inspect = Command::new("docker");
        inspect
            .current_dir(project)
            .args(["compose", "ps", "--status", "running", "--services"]);
        environment.apply(&mut inspect);
        let output = inspect.output()?;
        let already_running = output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|service| service == "postgres");
        if !already_running {
            let mut start = Command::new("docker");
            start
                .current_dir(project)
                .args(["compose", "up", "-d", "postgres"]);
            environment.apply(&mut start);
            let status = start.status()?;
            if !status.success() {
                return Err(io::Error::other(format!(
                    "docker compose exited with {status}"
                )));
            }
        }
        Ok(Self {
            project: Some(project.to_path_buf()),
            environment: Some(environment.clone()),
            started: !already_running,
        })
    }

    pub(super) fn stop(&mut self) -> io::Result<()> {
        if !self.started {
            return Ok(());
        }
        let project = self.project.as_ref().expect("managed database has project");
        let mut stop = Command::new("docker");
        stop.current_dir(project)
            .args(["compose", "stop", "postgres"]);
        self.environment
            .as_ref()
            .expect("managed database has environment")
            .apply(&mut stop);
        let status = stop.status()?;
        if status.success() {
            self.started = false;
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "docker compose stop exited with {status}"
            )))
        }
    }

    pub(super) fn update_environment(&mut self, environment: ProjectEnvironment) {
        if self.project.is_some() {
            self.environment = Some(environment);
        }
    }
}

impl Drop for ManagedDatabase {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn log_pipe(
    service: &'static str,
    pipe: Option<impl Read + Send + 'static>,
) -> Option<thread::JoinHandle<()>> {
    let pipe = pipe?;
    Some(thread::spawn(move || {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            eprintln!("[{service}] {line}");
        }
    }))
}

fn terminate(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    #[cfg(unix)]
    {
        let process_group = format!("-{}", child.id());
        let _ = Command::new("kill")
            .args(["-TERM", &process_group])
            .status();
        for _ in 0..20 {
            let parent_exited = child.try_wait().ok().flatten().is_some();
            if parent_exited && !process_group_alive(&process_group) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = Command::new("kill")
            .args(["-KILL", &process_group])
            .status();
        let _ = child.wait();
    }
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = child.wait();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(unix)]
fn process_group_alive(process_group: &str) -> bool {
    Command::new("kill")
        .args(["-0", process_group])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn isolate_process_group(command: &mut Command) {
    command.process_group(0);
}

#[cfg(windows)]
fn isolate_process_group(command: &mut Command) {
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn isolate_process_group(_command: &mut Command) {}

#[cfg(test)]
mod tests;
