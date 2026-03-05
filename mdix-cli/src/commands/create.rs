// mdix-cli/src/commands/create.rs

use std::path::PathBuf;
use clap::Args;
use serde::Serialize;
use crate::commands::{handle_error, GlobalOpts};
use crate::output::{json_output, printer};
use crate::services::file_io;

#[derive(Args)]
pub struct CreateArgs {
    /// Output .mdix file path
    pub file: PathBuf,

    /// Template: basic | advanced | security | dlm
    #[arg(long, default_value = "basic")]
    pub template: String,

    /// Overwrite if file already exists
    #[arg(long)]
    pub force: bool,
}

#[derive(Serialize)]
struct CreateOutput {
    file_path: String,
    template:  String,
}

pub fn run(args: CreateArgs, global: &GlobalOpts) -> i32 {
    if args.file.exists() && !args.force {
        let err = crate::commands::CliError::InvalidArgument(format!(
            "'{}' already exists. Use --force to overwrite.",
            args.file.display()
        ));
        return handle_error(&err, global.json);
    }

    let content = match build_template(&args.template) {
        Ok(c)  => c,
        Err(e) => {
            let err = crate::commands::CliError::InvalidArgument(e);
            return handle_error(&err, global.json);
        }
    };

    match file_io::write_file(&args.file, &content) {
        Ok(()) => {
            if global.json {
                json_output::print_result(CreateOutput {
                    file_path: args.file.to_string_lossy().to_string(),
                    template:  args.template.clone(),
                });
                return 0;
            }
            if !global.quiet {
                printer::success(&format!(
                    "Created {} (template: {})",
                    args.file.display(),
                    args.template
                ));
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
}

fn build_template(name: &str) -> Result<String, String> {
    match name {
        "basic" => Ok(TEMPLATE_BASIC.to_string()),
        "advanced" => Ok(TEMPLATE_ADVANCED.to_string()),
        "security" => Ok(TEMPLATE_SECURITY.to_string()),
        "dlm"      => Ok(TEMPLATE_DLM.to_string()),
        other => Err(format!(
            "Unknown template '{}'. Available: basic, advanced, security, dlm",
            other
        )),
    }
}

const TEMPLATE_BASIC: &str = r#"@CONFIG(
  version -> "1.0.0"
)

@DATA(
  app_name = "MyApp"
  version  = "1.0.0"
  port     = 8080
)
"#;

const TEMPLATE_ADVANCED: &str = r#"@CONFIG(
  version  -> "1.0.0"
  features -> "advanced"
)

@ENUMS(
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
  LogLevel    { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
)

@QUICKFUNCS(
  ~serverConfig<object>(env<enum>, host) {
    return {
      host    = host
      port    = 8080
      ssl     = env == Environment.PROD
    }
  }
)

@DATA(
  current_env<enum> = Environment.DEV
  log_level<enum>   = LogLevel.INFO

  servers::
    serverConfig(Environment.DEV,  "dev.local"),
    serverConfig(Environment.PROD, "prod.example.com")
)
"#;

const TEMPLATE_SECURITY: &str = r#"@CONFIG(
  version -> "1.0.0"
)

@DLM(
  DEncryptor.aes256
)

@DATA(
  api_key       = "replace_me"
  database_url  = "replace_me"
)

@SECURITY(
  encryption -> { mode = "password", algorithm = "aes256-gcm" }
)
"#;

const TEMPLATE_DLM: &str = r#"@CONFIG(
  version -> "1.0.0"
)

@DLM(
  DCompressor.gzip,
  DEncryptor.aes256
)

@DATA(
  app_name = "MyApp"
  port     = 8080
)

@SECURITY(
  encryption -> { mode = "keyfile", algorithm = "aes256-gcm" }
)
"#;
