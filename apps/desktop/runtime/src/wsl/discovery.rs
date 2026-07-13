#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslDistribution {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslOnlineDistribution {
    pub name: String,
    pub display_name: String,
}

pub fn discover_wsl_distributions() -> Result<Vec<WslDistribution>, String> {
    discover_platform_distributions()
}

pub fn discover_wsl_online_distributions() -> Result<Vec<WslOnlineDistribution>, String> {
    discover_platform_online_distributions()
}

#[cfg(target_os = "windows")]
fn discover_platform_distributions() -> Result<Vec<WslDistribution>, String> {
    let output = super::command()
        .args(["--list", "--quiet"])
        .output()
        .map_err(|error| format!("Unable to run wsl.exe: {error}"))?;
    if !output.status.success() {
        return Err(decode_wsl_output(&output.stderr).trim().to_string());
    }
    Ok(parse_wsl_distribution_output(&output.stdout))
}

#[cfg(target_os = "windows")]
fn discover_platform_online_distributions() -> Result<Vec<WslOnlineDistribution>, String> {
    let output = super::command()
        .args(["--list", "--online"])
        .output()
        .map_err(|error| format!("Unable to list online WSL distributions: {error}"))?;
    if !output.status.success() {
        return Err(decode_wsl_output(&output.stderr).trim().to_string());
    }
    Ok(parse_wsl_online_distribution_output(&output.stdout))
}

#[cfg(not(target_os = "windows"))]
fn discover_platform_distributions() -> Result<Vec<WslDistribution>, String> {
    Ok(Vec::new())
}

#[cfg(not(target_os = "windows"))]
fn discover_platform_online_distributions() -> Result<Vec<WslOnlineDistribution>, String> {
    Ok(Vec::new())
}

#[cfg(any(target_os = "windows", test))]
fn parse_wsl_distribution_output(output: &[u8]) -> Vec<WslDistribution> {
    let text = decode_wsl_output(output);
    let mut distributions = Vec::new();
    for line in text.lines() {
        let name = line
            .trim()
            .trim_start_matches('*')
            .trim()
            .trim_matches('\0');
        if name.is_empty()
            || distributions
                .iter()
                .any(|distribution: &WslDistribution| distribution.name == name)
        {
            continue;
        }
        distributions.push(WslDistribution {
            name: name.to_string(),
        });
    }
    distributions
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn decode_wsl_output(output: &[u8]) -> String {
    let utf16 = is_utf16_le_output(output);
    if !utf16 {
        return String::from_utf8_lossy(output).into_owned();
    }
    let offset = usize::from(output.starts_with(&[0xff, 0xfe])) * 2;
    let units = output[offset..]
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&units)
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn is_utf16_le_output(output: &[u8]) -> bool {
    if output.starts_with(&[0xff, 0xfe]) {
        return true;
    }
    let sample = &output[..output.len().min(256)];
    if sample
        .chunks_exact(2)
        .any(|pair| matches!(pair, [b'\r' | b'\n', 0]))
    {
        return true;
    }
    let pairs = sample.chunks_exact(2).count();
    pairs >= 4 && sample.chunks_exact(2).filter(|pair| pair[1] == 0).count() >= pairs / 4
}

#[cfg(any(target_os = "windows", test))]
fn parse_wsl_online_distribution_output(output: &[u8]) -> Vec<WslOnlineDistribution> {
    let text = decode_wsl_output(output);
    let mut found_header = false;
    let mut distributions = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_matches('\0');
        if !found_header {
            found_header = line.starts_with("NAME") && line.contains("FRIENDLY NAME");
            continue;
        }
        let Some((name, display_name)) = split_online_distribution_row(line) else {
            continue;
        };
        if distributions
            .iter()
            .any(|distribution: &WslOnlineDistribution| distribution.name == name)
        {
            continue;
        }
        distributions.push(WslOnlineDistribution {
            name: name.to_string(),
            display_name: display_name.to_string(),
        });
    }
    distributions
}

#[cfg(any(target_os = "windows", test))]
fn split_online_distribution_row(line: &str) -> Option<(&str, &str)> {
    let split_at = line.char_indices().find_map(|(index, character)| {
        if !character.is_whitespace() {
            return None;
        }
        let rest = &line[index..];
        let whitespace = rest
            .chars()
            .take_while(|character| character.is_whitespace())
            .count();
        (whitespace >= 2).then_some(index)
    })?;
    let name = line[..split_at].trim();
    let display_name = line[split_at..].trim();
    (!name.is_empty() && !display_name.is_empty()).then_some((name, display_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utf8_distribution_list() {
        assert_eq!(
            parse_wsl_distribution_output(b"Ubuntu\nDebian\n\n"),
            vec![
                WslDistribution {
                    name: "Ubuntu".to_string()
                },
                WslDistribution {
                    name: "Debian".to_string()
                }
            ]
        );
    }

    #[test]
    fn parses_utf16_distribution_list_and_default_marker() {
        let units = "* Ubuntu\r\nDebian\r\n".encode_utf16().collect::<Vec<_>>();
        let bytes = units
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            parse_wsl_distribution_output(&bytes),
            vec![
                WslDistribution {
                    name: "Ubuntu".to_string()
                },
                WslDistribution {
                    name: "Debian".to_string()
                }
            ]
        );
    }

    #[test]
    fn parses_online_distribution_catalog() {
        let output = b"The following distributions can be installed.\n\nNAME                            FRIENDLY NAME\nUbuntu-24.04                    Ubuntu 24.04 LTS\nDebian                          Debian GNU/Linux\n";
        assert_eq!(
            parse_wsl_online_distribution_output(output),
            vec![
                WslOnlineDistribution {
                    name: "Ubuntu-24.04".to_string(),
                    display_name: "Ubuntu 24.04 LTS".to_string(),
                },
                WslOnlineDistribution {
                    name: "Debian".to_string(),
                    display_name: "Debian GNU/Linux".to_string(),
                },
            ]
        );
    }

    #[test]
    fn decodes_utf16_catalog_with_localized_prefix_and_no_bom() {
        let output = "以下是可安装的有效分发列表。\r\nNAME              FRIENDLY NAME\r\nDebian            Debian GNU/Linux\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            parse_wsl_online_distribution_output(&output),
            vec![WslOnlineDistribution {
                name: "Debian".to_string(),
                display_name: "Debian GNU/Linux".to_string(),
            }]
        );
    }
}
