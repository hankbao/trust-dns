// mod.rs

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{bind_to_if4, bind_to_if6};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::{bind_to_if4, bind_to_if6};
