use clap::{CommandFactory, Parser};

const LEGACY_ALIAS_HELP: &str = "Legacy compatibility aliases: -cf, -bc, -sc, -gc, -lc, -ic";

#[derive(Debug, Parser)]
#[command(
    name = "envoy",
    about = "Envoy: Environment orchestration for applications",
    version = env!("ENVOY_VERSION"),
    after_help = LEGACY_ALIAS_HELP
)]
pub(crate) struct Cli {
    #[arg(long, help = "Open the envoy documentation in the default browser.")]
    pub docs: bool,

    #[arg(long, help = "List all available commands")]
    pub list: bool,

    #[arg(
        long,
        value_name = "COMMAND",
        help = "Show detailed information about a command"
    )]
    pub info: Option<String>,

    #[arg(
        long,
        value_name = "COMMAND",
        help = "Show the resolved executable path for a command"
    )]
    pub which: Option<String>,

    #[arg(
        long = "commands-file",
        value_name = "PATH",
        help = "Path to commands.json file (auto-detected if not specified)"
    )]
    pub commands_file: Option<String>,

    #[arg(
        long = "bundles-config",
        value_name = "PATH",
        help = "Path to bundles config file (auto-discovers from ENVOY_BNDL_ROOTS if not specified)"
    )]
    pub bundles_config: Option<String>,

    #[arg(
        long = "set-config",
        value_name = "KEY=VALUE",
        help = "Set a user config value and save it. Use KEY= to clear a setting."
    )]
    pub set_config: Option<String>,

    #[arg(
        long = "get-config",
        value_name = "KEY",
        num_args = 0..=1,
        default_missing_value = "",
        help = "Print one or all user config values and exit. Omit KEY to print all settings."
    )]
    pub get_config: Option<String>,

    #[arg(
        long = "list-configs",
        help = "List all known configurable settings with their descriptions and exit."
    )]
    pub list_configs: bool,

    #[arg(
        long = "ignore-config",
        help = "Bypass the user config for this invocation."
    )]
    pub ignore_config: bool,

    #[arg(
        long,
        short = 'e',
        value_name = "ENV_COMMAND",
        help = "Run the target command inside a different command's environment."
    )]
    pub env: Option<String>,

    #[arg(long, short = 'v', help = "Enable verbose logging")]
    pub verbose: bool,

    #[arg(
        long = "inherit-env",
        short = 'i',
        help = "Inherit the full system environment"
    )]
    pub inherit_env: bool,

    #[arg(
        long,
        value_name = "VAR",
        help = "Show how VAR is mutated through env file processing for COMMAND."
    )]
    pub trace: Option<String>,

    #[arg(
        long,
        value_name = "COMMAND",
        num_args = 0..=1,
        default_missing_value = "",
        help = "Show diagnostics: discovered bundles/commands, team/pipeline \
context, package cache status, VCS detection, and bundle-root reachability. \
Pass a COMMAND to also include its full environment resolution trace."
    )]
    pub diagnose: Option<String>,

    #[arg(help = "Command to execute")]
    pub command: Option<String>,

    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        help = "Arguments to pass to the command"
    )]
    pub args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValueExpectation {
    Required,
    Optional,
}

pub(crate) fn parse(argv: &[String]) -> Result<Cli, clap::Error> {
    let normalized = normalize_argv(argv);
    let canonical = canonicalize_legacy_aliases(&normalized);
    let mut full_argv = Vec::with_capacity(canonical.len() + 1);
    full_argv.push(String::from("envoy"));
    full_argv.extend(canonical);
    Cli::try_parse_from(full_argv)
}

pub(crate) fn print_help() -> Result<(), clap::Error> {
    let mut command = Cli::command();
    command.print_help()?;
    println!();
    Ok(())
}

pub(crate) fn normalize_argv(argv: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut command_seen = false;

    for token in argv {
        if command_seen {
            result.push(token.clone());
            continue;
        }

        if token.starts_with('-') && token.contains('=') {
            let mut parts = token.splitn(2, '=');
            let flag = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default();
            result.push(flag.to_string());
            result.push(value.to_string());
        } else {
            if !token.starts_with('-') {
                command_seen = true;
            }
            result.push(token.clone());
        }
    }

    result
}

fn canonicalize_legacy_aliases(argv: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(argv.len());
    let mut command_seen = false;
    let mut expecting_value = None;

    for token in argv {
        if command_seen {
            result.push(token.clone());
            continue;
        }

        if let Some(expectation) = expecting_value {
            match expectation {
                ValueExpectation::Required => {
                    result.push(token.clone());
                    expecting_value = None;
                    continue;
                }
                ValueExpectation::Optional if !token.starts_with('-') => {
                    result.push(token.clone());
                    expecting_value = None;
                    continue;
                }
                ValueExpectation::Optional => {
                    expecting_value = None;
                }
            }
        }

        if token == "--" {
            command_seen = true;
            result.push(token.clone());
            continue;
        }

        if let Some((canonical, expectation, attached_value)) = legacy_alias(token) {
            result.push(canonical.to_string());
            if let Some(value) = attached_value {
                result.push(value.to_string());
            } else {
                expecting_value = expectation;
            }
            continue;
        }

        result.push(token.clone());

        if let Some(expectation) = option_value_expectation(token) {
            expecting_value = Some(expectation);
        } else if !token.starts_with('-') {
            command_seen = true;
        }
    }

    result
}

fn legacy_alias(token: &str) -> Option<(&'static str, Option<ValueExpectation>, Option<&str>)> {
    const ALIASES: [(&str, &str, Option<ValueExpectation>); 6] = [
        ("-cf", "--commands-file", Some(ValueExpectation::Required)),
        ("-bc", "--bundles-config", Some(ValueExpectation::Required)),
        ("-sc", "--set-config", Some(ValueExpectation::Required)),
        ("-gc", "--get-config", Some(ValueExpectation::Optional)),
        ("-lc", "--list-configs", None),
        ("-ic", "--ignore-config", None),
    ];

    for (alias, canonical, expectation) in ALIASES {
        if token == alias {
            return Some((canonical, expectation, None));
        }

        if let Some(value) = token
            .strip_prefix(alias)
            .and_then(|tail| tail.strip_prefix('='))
        {
            return Some((canonical, None, Some(value)));
        }
    }

    None
}

fn option_value_expectation(token: &str) -> Option<ValueExpectation> {
    match token {
        "--info" | "--which" | "--commands-file" | "--bundles-config" | "--set-config"
        | "--env" | "-e" | "--trace" => Some(ValueExpectation::Required),
        "--get-config" | "--diagnose" => Some(ValueExpectation::Optional),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{canonicalize_legacy_aliases, normalize_argv};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn normalize_argv_expands_short_and_long_equals_forms() {
        let input = strings(&["-e=python", "--bundles-config=studio", "maya"]);
        let expected = strings(&["-e", "python", "--bundles-config", "studio", "maya"]);

        assert_eq!(normalize_argv(&input), expected);
    }

    #[test]
    fn normalize_argv_stops_at_first_non_option_token() {
        let input = strings(&["maya", "--trace=UE_PATH", "-sc=bundles_config=studio"]);

        assert_eq!(normalize_argv(&input), input);
    }

    #[test]
    fn normalize_argv_preserves_equals_in_option_values() {
        let input = strings(&["-sc=bundles_config=C:\\path=with=equals.json"]);
        let expected = strings(&["-sc", "bundles_config=C:\\path=with=equals.json"]);

        assert_eq!(normalize_argv(&input), expected);
    }

    #[test]
    fn normalize_argv_does_not_expand_child_process_args() {
        let input = strings(&["python", "-c", "x=1", "--flag=value"]);

        assert_eq!(normalize_argv(&input), input);
    }

    #[test]
    fn canonicalize_legacy_aliases_translates_multi_char_aliases() {
        let input = strings(&["-cf", "commands.json", "-lc"]);
        let expected = strings(&["--commands-file", "commands.json", "--list-configs"]);

        assert_eq!(canonicalize_legacy_aliases(&input), expected);
    }

    #[test]
    fn canonicalize_legacy_aliases_understands_option_values_before_command() {
        let input = strings(&["-e", "python", "-bc=studio", "maya", "-gc"]);
        let expected = strings(&["-e", "python", "--bundles-config", "studio", "maya", "-gc"]);

        assert_eq!(canonicalize_legacy_aliases(&input), expected);
    }
}
