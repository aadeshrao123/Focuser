//! Hidden terminal input for protection passwords. Prompts never use stdout,
//! which is reserved for the command result, including `--json` output.

use std::io::IsTerminal;

use anyhow::{Context, bail};

pub fn create() -> anyhow::Result<String> {
    require_terminal()?;
    confirmed_password(|prompt| rpassword::prompt_password(prompt).context("cannot read password"))
}

pub fn unlock() -> anyhow::Result<String> {
    require_terminal()?;
    rpassword::prompt_password("Password: ").context("cannot read password")
}

fn require_terminal() -> anyhow::Result<()> {
    // Do not open a controlling TTY behind a script's redirected stdin.
    if !std::io::stdin().is_terminal() {
        bail!(
            "password prompting requires an interactive terminal; supply an explicit password for noninteractive commands"
        );
    }
    Ok(())
}

fn confirmed_password(
    mut read: impl FnMut(&str) -> anyhow::Result<String>,
) -> anyhow::Result<String> {
    let password = read("Password: ")?;
    if password.is_empty() {
        bail!("password must not be empty");
    }
    let confirmation = read("Confirm password: ")?;
    if password != confirmation {
        bail!("passwords do not match; protection was not enabled");
    }
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_preserves_whitespace_in_passwords() {
        let password = confirmed_password(|_| Ok(" secret ".into())).unwrap();
        assert_eq!(password, " secret ");
    }

    #[test]
    fn mismatched_passwords_are_rejected_without_disclosing_them() {
        let mut values = ["first-secret", "second-secret"].into_iter();
        let error = confirmed_password(|_| Ok(values.next().unwrap().into())).unwrap_err();
        assert_eq!(
            error.to_string(),
            "passwords do not match; protection was not enabled"
        );
    }

    #[test]
    fn empty_password_does_not_request_confirmation() {
        let mut calls = 0;
        let error = confirmed_password(|_| {
            calls += 1;
            Ok(String::new())
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "password must not be empty");
        assert_eq!(calls, 1);
    }

    #[test]
    fn input_failure_is_propagated() {
        let error = confirmed_password(|_| bail!("input unavailable")).unwrap_err();
        assert_eq!(error.to_string(), "input unavailable");
    }
}
