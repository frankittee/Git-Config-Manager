use std::io::{BufRead, Write};

use anyhow::{Context, Result, bail};

use crate::profiles::Profile;

pub fn collect_profile(
    name: Option<String>,
    email: Option<String>,
    signing_key: Option<String>,
    ssh_host: Option<String>,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Profile> {
    let name = match name {
        Some(value) => value,
        None => prompt_required("Git author name: ", reader, writer)?,
    };
    let email = match email {
        Some(value) => value,
        None => prompt_required("Git author email: ", reader, writer)?,
    };
    let signing_key = match signing_key {
        Some(value) => Some(value),
        None if prompt_confirmation("Enable commit signing? [y/N]: ", reader, writer)? => {
            Some(prompt_required("Signing key: ", reader, writer)?)
        }
        None => None,
    };
    let ssh_host = match ssh_host {
        Some(value) => Some(value),
        None => prompt_optional("SSH host alias (leave blank to skip): ", reader, writer)?,
    };

    Ok(Profile {
        name,
        email,
        signing_key,
        ssh_host,
    })
}

pub fn edit_profile(
    current: Profile,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Profile> {
    let name = prompt_with_default("Git author name", &current.name, reader, writer)?;
    let email = prompt_with_default("Git author email", &current.email, reader, writer)?;
    let signing_enabled = prompt_confirmation_with_default(
        "Enable commit signing?",
        current.signing_key.is_some(),
        reader,
        writer,
    )?;
    let signing_key = if signing_enabled {
        match current.signing_key {
            Some(key) => Some(prompt_with_default("Signing key", &key, reader, writer)?),
            None => Some(prompt_required("Signing key: ", reader, writer)?),
        }
    } else {
        None
    };
    let ssh_host =
        prompt_optional_with_default("SSH host alias", current.ssh_host, reader, writer)?;

    Ok(Profile {
        name,
        email,
        signing_key,
        ssh_host,
    })
}

fn prompt_required(
    prompt: &str,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<String> {
    loop {
        write_prompt(prompt, writer)?;
        let value = read_line(reader)?;
        if !value.is_empty() {
            return Ok(value);
        }
        writeln!(writer, "Value must not be empty.").context("could not write prompt")?;
    }
}

fn prompt_optional(
    prompt: &str,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Option<String>> {
    write_prompt(prompt, writer)?;
    let value = read_line(reader)?;
    Ok((!value.is_empty()).then_some(value))
}

fn prompt_confirmation(
    prompt: &str,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<bool> {
    loop {
        write_prompt(prompt, writer)?;
        let value = read_line(reader)?;
        match value.to_ascii_lowercase().as_str() {
            "" | "n" | "no" => return Ok(false),
            "y" | "yes" => return Ok(true),
            _ => {
                writeln!(writer, "Please answer y or n.").context("could not write prompt")?;
            }
        }
    }
}

fn prompt_with_default(
    label: &str,
    default: &str,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<String> {
    write_prompt(&format!("{label} [{default}]: "), writer)?;
    let value = read_line(reader)?;
    if value.is_empty() {
        Ok(default.to_owned())
    } else {
        Ok(value)
    }
}

fn prompt_optional_with_default(
    label: &str,
    default: Option<String>,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Option<String>> {
    let suffix = default
        .as_deref()
        .map(|value| format!("{value}; blank keeps, - clears"))
        .unwrap_or_else(|| "leave blank to skip".to_owned());
    write_prompt(&format!("{label} [{suffix}]: "), writer)?;
    let value = read_line(reader)?;
    Ok(if value == "-" {
        None
    } else if value.is_empty() {
        default
    } else {
        Some(value)
    })
}

fn prompt_confirmation_with_default(
    label: &str,
    default: bool,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<bool> {
    let suffix = if default { "[Y/n]" } else { "[y/N]" };
    loop {
        write_prompt(&format!("{label} {suffix}: "), writer)?;
        let value = read_line(reader)?;
        match value.to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "n" | "no" => return Ok(false),
            "y" | "yes" => return Ok(true),
            _ => {
                writeln!(writer, "Please answer y or n.").context("could not write prompt")?;
            }
        }
    }
}

fn write_prompt(prompt: &str, writer: &mut impl Write) -> Result<()> {
    write!(writer, "{prompt}").context("could not write prompt")?;
    writer.flush().context("could not display prompt")
}

fn read_line(reader: &mut impl BufRead) -> Result<String> {
    let mut input = String::new();
    let bytes = reader
        .read_line(&mut input)
        .context("could not read interactive input")?;
    if bytes == 0 {
        bail!("interactive input ended before the profile was complete");
    }
    Ok(input.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn collect(
        name: Option<&str>,
        email: Option<&str>,
        signing_key: Option<&str>,
        ssh_host: Option<&str>,
        input: &str,
    ) -> (Result<Profile>, String) {
        let mut reader = Cursor::new(input.as_bytes());
        let mut output = Vec::new();
        let result = collect_profile(
            name.map(str::to_owned),
            email.map(str::to_owned),
            signing_key.map(str::to_owned),
            ssh_host.map(str::to_owned),
            &mut reader,
            &mut output,
        );
        (result, String::from_utf8(output).unwrap())
    }

    #[test]
    fn collects_unsigned_profile_in_order() {
        let (result, output) = collect(None, None, None, None, " Ada \n ada@example.com \n\n\n");
        assert_eq!(
            result.unwrap(),
            Profile {
                name: "Ada".into(),
                email: "ada@example.com".into(),
                signing_key: None,
                ssh_host: None,
            }
        );
        assert_eq!(
            output,
            "Git author name: Git author email: Enable commit signing? [y/N]: SSH host alias (leave blank to skip): "
        );
    }

    #[test]
    fn only_prompts_for_missing_required_values() {
        let (result, output) = collect(Some("Ada"), None, None, None, "ada@example.com\nno\n\n");
        assert_eq!(result.unwrap().email, "ada@example.com");
        assert_eq!(
            output,
            "Git author email: Enable commit signing? [y/N]: SSH host alias (leave blank to skip): "
        );
    }

    #[test]
    fn collects_signing_key_after_confirmation() {
        let (result, output) =
            collect(None, None, None, None, "Ada\na@example.com\nYES\n KEY \n\n");
        assert_eq!(result.unwrap().signing_key.as_deref(), Some("KEY"));
        assert!(output.contains("Signing key: "));
    }

    #[test]
    fn retries_empty_required_and_invalid_confirmation() {
        let (result, output) = collect(
            None,
            None,
            None,
            None,
            "\nAda\n\na@example.com\nmaybe\ny\nKEY\n\n",
        );
        assert!(result.is_ok());
        assert_eq!(output.matches("Git author name: ").count(), 2);
        assert_eq!(output.matches("Git author email: ").count(), 2);
        assert_eq!(output.matches("Enable commit signing? [y/N]: ").count(), 2);
        assert!(output.contains("Value must not be empty."));
        assert!(output.contains("Please answer y or n."));
    }

    #[test]
    fn provided_signing_key_skips_confirmation() {
        let (result, output) = collect(None, None, Some("KEY"), None, "Ada\na@example.com\n\n");
        assert_eq!(result.unwrap().signing_key.as_deref(), Some("KEY"));
        assert!(!output.contains("Enable commit signing?"));
    }

    #[test]
    fn eof_cancels_collection() {
        let (result, _) = collect(None, None, None, None, "Ada\n");
        assert!(result.unwrap_err().to_string().contains("input ended"));
    }

    #[test]
    fn edit_keeps_defaults_on_empty_input() {
        let current = Profile {
            name: "Ada".into(),
            email: "ada@example.com".into(),
            signing_key: Some("KEY".into()),
            ssh_host: None,
        };
        let mut reader = Cursor::new(b"\n\n\n\n\n");
        let mut output = Vec::new();
        let edited = edit_profile(current.clone(), &mut reader, &mut output).unwrap();

        assert_eq!(edited, current);
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Git author name [Ada]: "));
        assert!(output.contains("Enable commit signing? [Y/n]: "));
        assert!(output.contains("Signing key [KEY]: "));
    }

    #[test]
    fn edit_can_change_values_and_disable_signing() {
        let current = Profile {
            name: "Old".into(),
            email: "old@example.com".into(),
            signing_key: Some("KEY".into()),
            ssh_host: None,
        };
        let mut reader = Cursor::new(b"New\nnew@example.com\nno\n\n");
        let mut output = Vec::new();
        let edited = edit_profile(current, &mut reader, &mut output).unwrap();

        assert_eq!(edited.name, "New");
        assert_eq!(edited.email, "new@example.com");
        assert_eq!(edited.signing_key, None);
    }
}
