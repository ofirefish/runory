//! `runory-plugin` — Plugin API 1.0 developer CLI.
//!
//! Commands: init / validate / pack / inspect / keygen / sign

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use runory_lib::plugin_sdk::{
    init_plugin, inspect_plugin_package, keygen, pack_plugin_dir, sign_plugin_package,
    validate_plugin_dir, CliError,
};

#[derive(Debug, Parser)]
#[command(name = "runory-plugin")]
#[command(about = "Runory Plugin API 1.0 developer CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Scaffold a new Plugin API 1 plugin
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long = "publisher-id")]
        publisher_id: Option<String>,
        #[arg(long = "publisher-name")]
        publisher_name: Option<String>,
        #[arg(long = "name")]
        display_name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Workspace category: terminal / files / dashboard / operations / deployment / none
        #[arg(long)]
        category: Option<String>,
        /// Package-relative logo path (PNG/WebP)
        #[arg(long, default_value = "assets/logo.png")]
        logo: String,
        #[arg(long)]
        yes: bool,
    },
    Validate {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Pack {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    Inspect {
        package: PathBuf,
    },
    /// Generate an Ed25519 publisher signing keypair
    Keygen {
        #[arg(long)]
        publisher: String,
        #[arg(long = "key-id")]
        key_id: String,
        #[arg(long, default_value = "publisher-key.json")]
        out: PathBuf,
        #[arg(long = "public-out")]
        public_out: Option<PathBuf>,
        #[arg(long)]
        password: Option<String>,
    },
    /// Sign a .runory-plugin package (embeds META-INF/signature.json)
    Sign {
        package: PathBuf,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        password: Option<String>,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error [{}]: {err}", err.code());
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init {
            path,
            id,
            publisher_id,
            publisher_name,
            display_name,
            description,
            category,
            logo,
            yes,
        } => {
            let plugin_id = resolve_value("Plugin ID", id, yes, "example.hello")?;
            let publisher_id = resolve_value("Publisher ID", publisher_id, yes, "example")?;
            let publisher_name =
                resolve_value("Publisher name", publisher_name, yes, &publisher_id)?;
            let display_name =
                resolve_value("Plugin name", display_name, yes, &humanize_id(&plugin_id))?;
            let description = resolve_value("Description", description, yes, "A Runory plugin")?;
            let category_raw = resolve_value(
                "Workspace category (terminal/files/dashboard/operations/deployment/none)",
                category,
                yes,
                "none",
            )?;
            let category = match category_raw.trim().to_ascii_lowercase().as_str() {
                "" | "none" => None,
                "terminal" | "files" | "dashboard" | "operations" | "deployment" => {
                    Some(category_raw.trim().to_ascii_lowercase())
                }
                other => {
                    return Err(CliError::InvalidManifest(format!(
                        "unsupported workspace category `{other}`"
                    )));
                }
            };
            let logo_path = if category.is_some() {
                Some(resolve_value(
                    "Logo path",
                    Some(logo),
                    yes,
                    "assets/logo.png",
                )?)
            } else {
                None
            };
            let target = if path == PathBuf::from(".") {
                PathBuf::from(plugin_id.replace('.', "-"))
            } else {
                path
            };
            let created = init_plugin(
                &target,
                &plugin_id,
                &publisher_id,
                &publisher_name,
                &display_name,
                &description,
                category.as_deref(),
                logo_path.as_deref(),
            )?;
            println!("Created plugin at {}", created.display());
            println!(
                "Next: cd {} && runory-plugin validate && runory-plugin pack",
                created.display()
            );
            Ok(())
        }
        Commands::Validate { path } => {
            let report = validate_plugin_dir(&path)?;
            for message in &report.messages {
                if message == "Plugin is valid." {
                    println!("{message}");
                } else {
                    println!("✓ {message}");
                }
            }
            Ok(())
        }
        Commands::Pack { path, output } => {
            let packed = pack_plugin_dir(&path, output.as_deref())?;
            println!("Packed {}", packed.output_path.display());
            println!("Plugin  {}@{}", packed.plugin_id, packed.version);
            println!("Files   {}", packed.file_count);
            println!("SHA-256 {}", packed.sha256_hex);
            Ok(())
        }
        Commands::Inspect { package } => {
            let report = inspect_plugin_package(&package)?;
            println!("Plugin          {}", report.plugin_id);
            println!("Version         {}", report.version);
            println!("Name            {}", report.name);
            println!(
                "Publisher       {} ({})",
                report.publisher_name, report.publisher_id
            );
            println!("API version     {}", report.api_version);
            println!(
                "Logo            {}",
                report.logo.as_deref().unwrap_or("(none)")
            );
            println!(
                "Workspace cat.  {}",
                report.category.as_deref().unwrap_or("(none)")
            );
            if report.workspace_entries.is_empty() {
                println!("Workspace ents. (none)");
            } else {
                println!("Workspace ents. {}", report.workspace_entries.join(", "));
            }
            println!("Permissions     {}", report.permissions.join(", "));
            println!("Workflows       {}", report.workflows.join(", "));
            println!("Package SHA-256 {}", report.sha256_hex);
            println!("Signature       {}", report.signature_status);
            println!("Files           {}", report.file_count);
            Ok(())
        }
        Commands::Keygen {
            publisher,
            key_id,
            out,
            public_out,
            password,
        } => {
            let public = public_out.unwrap_or_else(|| {
                let mut p = out.clone();
                p.set_extension("pub.json");
                p
            });
            keygen(
                &publisher,
                &key_id,
                &out,
                Some(&public),
                password.as_deref(),
            )?;
            println!("Wrote private key {}", out.display());
            println!("Wrote public key  {}", public.display());
            println!("Never share or log the private key.");
            Ok(())
        }
        Commands::Sign {
            package,
            key,
            password,
        } => {
            let digest = sign_plugin_package(&package, &key, password.as_deref())?;
            println!("Signed {}", package.display());
            println!("Package digest {}", digest);
            Ok(())
        }
    }
}

fn resolve_value(
    label: &str,
    provided: Option<String>,
    yes: bool,
    default: &str,
) -> Result<String, CliError> {
    if let Some(value) = provided {
        let trimmed = value.trim().to_owned();
        if trimmed.is_empty() {
            return Err(CliError::InvalidManifest(format!(
                "{label} must be non-empty"
            )));
        }
        return Ok(trimmed);
    }
    if yes {
        return Ok(default.to_owned());
    }
    print!("{label} [{default}]: ");
    let _ = io::stdout().flush();
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|_| CliError::Storage)?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        Ok(default.to_owned())
    } else {
        Ok(trimmed.to_owned())
    }
}

fn humanize_id(id: &str) -> String {
    id.split('.')
        .last()
        .unwrap_or(id)
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
