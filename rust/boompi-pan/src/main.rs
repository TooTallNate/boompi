#[cfg(any(target_os = "linux", test))]
mod pan;

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    use std::time::Duration;
    use tokio::signal::unix::{signal, SignalKind};

    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let shutdown = async {
        tokio::select! {
            _ = terminate.recv() => {},
            _ = interrupt.recv() => {},
        }
    };
    tokio::pin!(shutdown);

    loop {
        let connection = tokio::select! {
            biased;
            _ = &mut shutdown => return Ok(()),
            result = zbus::connection::Builder::system()?
                .method_timeout(Duration::from_secs(10)).build() => result,
        };
        match connection {
            Ok(connection) => {
                let stopping = tokio::select! {
                    biased;
                    _ = &mut shutdown => true,
                    result = pan::serve(&connection, Duration::from_secs(15)) => {
                        eprintln!("boompi-pan: D-Bus tracking stopped: {result:?}");
                        false
                    },
                };
                // Closing this connection releases only our registrations, even
                // when a Register reply was lost. Never unregister by UUID.
                if let Err(error) = connection.close().await {
                    eprintln!("boompi-pan: closing D-Bus connection: {error}");
                }
                if stopping {
                    return Ok(());
                }
            }
            Err(error) => eprintln!("boompi-pan: connecting to system D-Bus: {error}"),
        }
        tokio::select! {
            biased;
            _ = &mut shutdown => return Ok(()),
            _ = tokio::time::sleep(Duration::from_secs(5)) => {},
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("boompi-pan requires Linux with BlueZ and a pre-created br-pan bridge")
}
