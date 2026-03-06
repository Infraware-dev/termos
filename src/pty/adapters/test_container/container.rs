//! Container builder for debian latest slim container image.

use std::pin::Pin;

use bollard::Docker;
use bollard::config::ContainerCreateBody;
use bollard::container::LogOutput;
use bollard::query_parameters::{
    AttachContainerOptionsBuilder, CreateContainerOptionsBuilder, CreateImageOptionsBuilder,
    RemoveContainerOptionsBuilder, ResizeContainerTTYOptionsBuilder,
};
use futures::{Stream, StreamExt};
use tokio::io::AsyncWrite;

/// Container IO streams for interacting with the container's stdin, stdout, and stderr.
pub struct IoHandles {
    pub input: Pin<Box<dyn AsyncWrite + Send>>,
    pub output: Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>,
}

/// Builder for creating and managing a Debian container using [bollard](https://docs.rs/bollard/latest/bollard/).
pub struct Container {
    docker: Docker,
    name: String,
}

impl Container {
    /// Sets up a Debian container using bollard and returns a [handle to it](self::Container).
    ///
    /// ## Summary flow
    ///
    /// ```text
    ///   connect_with_local_defaults()
    ///           │
    ///      create_image()          ← pull debian:bookworm-slim
    ///           │
    ///      create_container()      ← tty=true, open_stdin=true, cmd=[/bin/bash]
    ///           │
    ///      start_container()
    ///           │
    ///      attach_container()      ← stdin+stdout+stderr, stream=true
    ///           │
    ///       ┌───┴───┐
    ///       │       │
    ///     output  input            ← async Stream / AsyncWrite
    ///       │       │
    ///     adapt   adapt            ← bridge to sync Read/Write
    ///       │       │
    ///    PtyReader PtyWriter       ← plug into PtySession trait
    /// ```
    pub async fn setup() -> anyhow::Result<(Container, IoHandles)> {
        let docker = Docker::connect_with_local_defaults()?;
        let name = format!("infraware_{}", uuid::Uuid::new_v4());
        let container = Container { docker, name };
        container.pull_image().await?;
        container.create_container().await?;
        container.start_container().await?;
        let container_io = container.attach_container().await?;

        Ok((container, container_io))
    }

    /// Resizes the container's TTY to the given dimensions.
    pub async fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let opts = ResizeContainerTTYOptionsBuilder::default()
            .w(i32::from(cols))
            .h(i32::from(rows))
            .build();
        self.docker.resize_container_tty(&self.name, opts).await?;
        Ok(())
    }

    /// Returns `true` if the container is currently running.
    pub async fn is_running(&self) -> bool {
        self.docker
            .inspect_container(&self.name, None)
            .await
            .ok()
            .and_then(|info| info.state)
            .and_then(|state| state.running)
            .unwrap_or(false)
    }

    /// Stops and removes the container to clean up resources after use.
    ///
    /// Stop is best-effort: even if the stop call fails (e.g., container
    /// already exited or a transient network error), removal is always
    /// attempted with `force(true)` which tells Docker to kill and remove
    /// in one shot.
    pub async fn stop(&self) -> anyhow::Result<()> {
        tracing::debug!("Stopping container {}", self.name);
        if let Err(e) = self.docker.stop_container(&self.name, None).await {
            tracing::debug!(
                "Stop request for container {} returned error (will still attempt removal): {e}",
                self.name
            );
        } else {
            tracing::debug!("Stopped container {}", self.name);
        }

        let opts = RemoveContainerOptionsBuilder::default().force(true).build();
        tracing::debug!("Removing container {} with options: {:?}", self.name, opts);
        self.docker.remove_container(&self.name, Some(opts)).await?;
        tracing::debug!("Removed container {}", self.name);

        Ok(())
    }

    /// Create the container image by pulling it from the registry if not already present.
    async fn pull_image(&self) -> anyhow::Result<()> {
        let options = CreateImageOptionsBuilder::default()
            .from_image("debian")
            .tag("bookworm-slim")
            .build();
        tracing::debug!("Pulling Debian image image: {options:?}");
        let mut pull_stream = self.docker.create_image(Some(options), None, None);

        let mut image_info = None;
        while let Some(token) = pull_stream.next().await {
            let info = token?;
            image_info = Some(info);
            tracing::debug!("Pulling image... progress: {image_info:?}");
        }
        let Some(image_info) = image_info else {
            return Err(anyhow::anyhow!(
                "Failed to pull image: no information received"
            ));
        };
        tracing::debug!("Image pulled; image info: {image_info:?}");

        Ok(())
    }

    /// Create the container with the appropriate configuration (tty, open_stdin, cmd).
    ///
    /// Returns the container name which can be used to start and attach to the container later.
    async fn create_container(&self) -> anyhow::Result<()> {
        tracing::debug!("Creating Container: {}", self.name);

        let options = CreateContainerOptionsBuilder::default()
            .name(&self.name)
            .build();

        let config = ContainerCreateBody {
            image: Some("debian:bookworm-slim".to_string()),
            tty: Some(true),
            open_stdin: Some(true),
            cmd: Some(vec!["/bin/bash".to_string()]),
            ..Default::default()
        };

        self.docker.create_container(Some(options), config).await?;
        tracing::debug!("Created container: {}", self.name);
        Ok(())
    }

    /// Start the container so that it can be attached to for interactive command execution.
    async fn start_container(&self) -> anyhow::Result<()> {
        tracing::debug!("Starting container: {}", self.name);
        self.docker.start_container(&self.name, None).await?;
        tracing::debug!("Started container: {}", self.name);

        Ok(())
    }

    /// Attach to the container's stdin, stdout, and stderr streams for interactive command execution.
    async fn attach_container(&self) -> anyhow::Result<IoHandles> {
        let options = AttachContainerOptionsBuilder::default()
            .stderr(true)
            .stdout(true)
            .stdin(true)
            .stream(true)
            .build();
        tracing::debug!("Attaching container: {options:?}");
        let attach = self
            .docker
            .attach_container(&self.name, Some(options))
            .await?;
        tracing::debug!("Attached container: {}", self.name);

        Ok(IoHandles {
            input: attach.input,
            output: attach.output,
        })
    }
}
