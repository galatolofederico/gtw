//! asks feral gamemode for the performance state while the game runs, on linux.
//! the library is opened at run time, so nothing happens without it. FABLE_NO_GAMEMODE=1 skips it.

/// keeps gamemode on while it lives.
pub struct GameMode {
    #[cfg(target_os = "linux")]
    lib: Option<libloading::Library>,
}

impl GameMode {
    /// asks for gamemode, never fails.
    pub fn request() -> Self {
        #[cfg(target_os = "linux")]
        {
            if std::env::var_os("FABLE_NO_GAMEMODE").is_some() {
                return Self { lib: None };
            }
            // safety: gamemode's request functions take nothing and return an int.
            let lib = unsafe { libloading::Library::new("libgamemode.so.0") }.ok();
            let lib = lib.and_then(|l| {
                let start = unsafe { l.get::<unsafe extern "C" fn() -> i32>(b"real_gamemode_request_start\0") }.ok()?;
                let r = unsafe { start() };
                drop(start);
                if r == 0 {
                    log::info!("GameMode on");
                    Some(l)
                } else {
                    log::info!("GameMode is installed but its daemon declined ({r})");
                    None
                }
            });
            Self { lib }
        }
        #[cfg(not(target_os = "linux"))]
        Self {}
    }
}

impl Drop for GameMode {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(l) = &self.lib {
            // the daemon also lets go of a process that exits.
            if let Ok(end) = unsafe { l.get::<unsafe extern "C" fn() -> i32>(b"real_gamemode_request_end\0") } {
                unsafe { end() };
            }
        }
    }
}
