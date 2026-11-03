//! Power actions used by the queue-drained shutdown countdown.
//!
//! The real implementation is deliberately small and the plan is inspectable so
//! tests (and a future simulation UI) can verify what would run without
//! touching the machine.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    Shutdown,
    Hibernate,
    Sleep,
    ExitApplication,
}

impl PowerAction {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "shutdown windows" | "shutdown" => Self::Shutdown,
            "hibernate" => Self::Hibernate,
            "sleep" => Self::Sleep,
            _ => Self::ExitApplication,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Shutdown => "Shutdown Windows",
            Self::Hibernate => "Hibernate",
            Self::Sleep => "Sleep",
            Self::ExitApplication => "Exit",
        }
    }

    pub fn plan(self) -> Plan {
        match self {
            Self::Shutdown => Plan::Command {
                program: "shutdown.exe",
                args: &["/s", "/t", "0"],
            },
            Self::Hibernate => Plan::Command {
                program: "shutdown.exe",
                args: &["/h"],
            },
            Self::Sleep => Plan::Suspend,
            Self::ExitApplication => Plan::ExitApplication,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    Command {
        program: &'static str,
        args: &'static [&'static str],
    },
    Suspend,
    ExitApplication,
}

impl Plan {
    pub fn command_line(self) -> String {
        match self {
            Self::Command { program, args } => format!("{program} {}", args.join(" ")),
            Self::Suspend => "SetSuspendState(0, 0, 0)".into(),
            Self::ExitApplication => "Exit NewsQueue".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownMode {
    Real,
    Mock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShutdownOutcome {
    Executed(String),
    Mocked(String),
    ExitApplication,
}

pub fn execute(action: PowerAction, mode: ShutdownMode) -> Result<ShutdownOutcome, String> {
    let plan = action.plan();
    if plan == Plan::ExitApplication {
        return Ok(ShutdownOutcome::ExitApplication);
    }
    if mode == ShutdownMode::Mock {
        return Ok(ShutdownOutcome::Mocked(plan.command_line()));
    }
    match plan {
        Plan::Command { program, args } => {
            let status = std::process::Command::new(program)
                .args(args)
                .status()
                .map_err(|error| format!("Cannot run {program}: {error}"))?;
            if status.success() {
                Ok(ShutdownOutcome::Executed(plan.command_line()))
            } else {
                Err(format!("{program} failed with {status}"))
            }
        }
        Plan::Suspend => {
            suspend()?;
            Ok(ShutdownOutcome::Executed(plan.command_line()))
        }
        Plan::ExitApplication => Ok(ShutdownOutcome::ExitApplication),
    }
}

#[cfg(target_os = "windows")]
fn suspend() -> Result<(), String> {
    #[link(name = "powrprof")]
    extern "system" {
        fn SetSuspendState(hibernate: i32, force: i32, disable_wake: i32) -> i32;
    }
    let ok = unsafe { SetSuspendState(0, 0, 0) };
    if ok != 0 {
        Ok(())
    } else {
        Err("Windows rejected the sleep request".into())
    }
}

#[cfg(not(target_os = "windows"))]
fn suspend() -> Result<(), String> {
    Err("Sleep is only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_map_to_power_actions() {
        assert_eq!(
            PowerAction::from_setting("Shutdown Windows"),
            PowerAction::Shutdown
        );
        assert_eq!(
            PowerAction::from_setting("Hibernate"),
            PowerAction::Hibernate
        );
        assert_eq!(PowerAction::from_setting("Sleep"), PowerAction::Sleep);
        assert_eq!(
            PowerAction::from_setting("Exit"),
            PowerAction::ExitApplication
        );
        assert_eq!(
            PowerAction::from_setting("unknown"),
            PowerAction::ExitApplication
        );
    }

    #[test]
    fn plans_are_inspectable_without_running_them() {
        assert_eq!(
            PowerAction::Shutdown.plan().command_line(),
            "shutdown.exe /s /t 0"
        );
        assert_eq!(
            PowerAction::Hibernate.plan().command_line(),
            "shutdown.exe /h"
        );
        assert_eq!(
            PowerAction::Sleep.plan().command_line(),
            "SetSuspendState(0, 0, 0)"
        );
        assert_eq!(
            PowerAction::ExitApplication.plan(),
            Plan::ExitApplication
        );
    }

    #[test]
    fn mock_mode_never_executes_a_power_command() {
        for action in [
            PowerAction::Shutdown,
            PowerAction::Hibernate,
            PowerAction::Sleep,
        ] {
            let outcome = execute(action, ShutdownMode::Mock).unwrap();
            assert!(matches!(outcome, ShutdownOutcome::Mocked(_)), "{outcome:?}");
        }
        assert_eq!(
            execute(PowerAction::ExitApplication, ShutdownMode::Mock).unwrap(),
            ShutdownOutcome::ExitApplication
        );
    }
}
