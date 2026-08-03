//! Phase 0 spike: validate Slint rendering + touch on the boombox hardware.
//!
//! What it shows / verifies:
//! - corner labels TL/TR/BL/BR → screen orientation (HyperPixel is
//!   `rotate=270`; check whether panel orientation is honored, else try
//!   `SLINT_KMS_ROTATION=90|180|270`)
//! - resolution readout → expected 800×480 landscape on the HyperPixel
//! - continuously animating bar → rendering smoothness / frame pacing
//! - crosshair + tap counter → touch input works and axes map correctly
//!   in every corner
//!
//! Laptop preview:    cargo run -p kms-test
//! On the Pi (CPU):   cargo build --release -p kms-test --no-default-features --features kms
//! On the Pi (GLES):  cargo build --release -p kms-test --no-default-features --features kms-gl
//! Run on the Pi from a console TTY: sudo ./kms-test  (needs /dev/dri + /dev/input)

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = TestWindow::new()?;

    // Liveness counter (also proves the event loop timer path works).
    let start = std::time::Instant::now();
    let timer = slint::Timer::default();
    {
        let weak = ui.as_weak();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(500),
            move || {
                if let Some(ui) = weak.upgrade() {
                    ui.set_info(
                        format!("event loop alive — {}s", start.elapsed().as_secs()).into(),
                    );
                    let size = ui.window().size();
                    ui.set_resolution(format!("{} × {}", size.width, size.height).into());
                }
            },
        );
    }

    ui.run()
}
