//! PPTUI language support for Zed.
//!
//! Embeds the deck format's XSD and injects a default `xml.fileAssociations`
//! entry, so LemMinX validates `.pptui` files without any user configuration.
//!
//! The schema is compiled into the wasm and written into the extension's
//! working directory on startup: dev extensions run with cwd
//! `extensions/work/<id>`, which contains no source files, so a repo-relative
//! path would not resolve for LemMinX.

use std::fs;
use std::path::Path;

use zed_extension_api::{
    self as zed,
    serde_json::{json, Value},
    settings::LspSettings,
    LanguageServerId, Result, Worktree,
};

/// Must match `[language_servers.pptui-lemminx]` in `extension.toml`.
const SERVER_ID: &str = "pptui-lemminx";
/// Where LemMinX releases are published.
const LS_REPO: &str = "redhat-developer/vscode-xml";
/// Where the schema is materialized, relative to the working directory.
const XSD_PATH: &str = "schemas/pptui.xsd";
/// The schema itself, embedded so it does not depend on the extension's source
/// files being reachable at runtime.
const PPTUI_XSD: &str = include_str!("../schemas/pptui.xsd");

struct PptuiExtension {
    cached_binary_path: Option<String>,
}

impl PptuiExtension {
    fn binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<String> {
        // 1) Explicit user override: `lsp.pptui-lemminx.binary.path`.
        if let Some(path) = lsp_settings(worktree)
            .and_then(|s| s.binary)
            .and_then(|b| b.path)
        {
            return Ok(path);
        }

        // 2) A LemMinX the user already has on PATH.
        if let Some(path) = worktree.which("lemminx") {
            return Ok(path);
        }

        // 3) A binary this extension downloaded earlier in the session.
        if let Some(path) = &self.cached_binary_path {
            if is_file(path) {
                return Ok(path.clone());
            }
        }

        // 4) Download a release into the extension's working directory.
        let path = install(language_server_id)?;
        self.cached_binary_path = Some(path.clone());
        Ok(path)
    }
}

impl zed::Extension for PptuiExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        let binary = lsp_settings(worktree).and_then(|s| s.binary);
        let (path, args, env) = match binary {
            Some(binary) => (binary.path, binary.arguments, binary.env),
            None => (None, None, None),
        };

        let command = match path {
            Some(path) => path,
            None => self.binary_path(language_server_id, worktree)?,
        };

        Ok(zed::Command {
            command,
            args: args.unwrap_or_default(),
            env: env.unwrap_or_default().into_iter().collect(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        // A user-provided `initialization_options` block is passed through untouched.
        if let Some(raw) = lsp_settings(worktree).and_then(|s| s.initialization_options) {
            return Ok(Some(raw));
        }

        // LemMinX only reads settings from `initializationOptions.settings`.
        Ok(Some(
            json!({ "settings": { "xml": xml_settings(worktree) } }),
        ))
    }

    fn language_server_workspace_configuration(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        // `workspace/configuration` asks for the `xml` section directly.
        Ok(Some(json!({ "xml": xml_settings(worktree) })))
    }
}

zed::register_extension!(PptuiExtension);

fn lsp_settings(worktree: &Worktree) -> Option<LspSettings> {
    LspSettings::for_worktree(SERVER_ID, worktree).ok()
}

/// Built-in defaults, overridden key by key by the user's `settings.xml`.
fn xml_settings(worktree: &Worktree) -> Value {
    let user = lsp_settings(worktree)
        .and_then(|s| s.settings)
        .and_then(|s| s.get("xml").cloned());

    match user {
        Some(user) => merge(default_xml(), user),
        None => default_xml(),
    }
}

/// Associate the deck file names with the bundled schema.
fn default_xml() -> Value {
    // If the working directory is unavailable, fall back to the relative path;
    // LemMinX then reports a clear error instead of silently skipping validation.
    let xsd = xsd_absolute_path().unwrap_or_else(|| XSD_PATH.to_string());
    let associations: Vec<Value> = ["**/*.pptui", "**/*.pptui.xml"]
        .iter()
        .map(|pattern| json!({ "pattern": pattern, "systemId": xsd.clone() }))
        .collect();

    json!({
        "validation": { "enabled": true },
        "fileAssociations": associations,
    })
}

/// LemMinX resolves `systemId` against its own cwd, so the path must be
/// absolute. Returns `None` if the working directory is unavailable.
fn xsd_absolute_path() -> Option<String> {
    let path = std::env::current_dir().ok()?.join(XSD_PATH);
    materialize_schema(&path)?;
    Some(path.to_string_lossy().into_owned())
}

/// Write the embedded schema to `path` unless an identical copy is already there.
fn materialize_schema(path: &Path) -> Option<()> {
    let up_to_date = fs::read_to_string(path)
        .map(|existing| existing == PPTUI_XSD)
        .unwrap_or(false);
    if up_to_date {
        return Some(());
    }

    fs::create_dir_all(path.parent()?).ok()?;
    fs::write(path, PPTUI_XSD).ok()?;
    Some(())
}

/// Deep merge where the user's values win; arrays are appended to.
fn merge(defaults: Value, user: Value) -> Value {
    match (defaults, user) {
        (Value::Object(mut defaults), Value::Object(user)) => {
            for (key, value) in user {
                let merged = match defaults.remove(&key) {
                    Some(existing) => merge(existing, value),
                    None => value,
                };
                defaults.insert(key, merged);
            }
            Value::Object(defaults)
        }
        (Value::Array(mut defaults), Value::Array(user)) => {
            defaults.extend(user);
            Value::Array(defaults)
        }
        (_, user) => user,
    }
}

fn install(language_server_id: &LanguageServerId) -> Result<String> {
    let (platform, arch) = zed::current_platform();
    let asset_stem = asset_stem(platform, arch)?;
    let binary_name = if matches!(platform, zed::Os::Windows) {
        format!("{asset_stem}.exe")
    } else {
        asset_stem.to_string()
    };

    // Reuse any release downloaded earlier (also works offline).
    if let Some(path) = find_installed_binary(&binary_name) {
        return Ok(path);
    }

    zed::set_language_server_installation_status(
        language_server_id,
        &zed::LanguageServerInstallationStatus::CheckingForUpdate,
    );
    let release = zed::latest_github_release(
        LS_REPO,
        zed::GithubReleaseOptions {
            require_assets: true,
            pre_release: false,
        },
    )?;

    let version_dir = format!("lemminx-{}", release.version);
    let binary_path = format!("{version_dir}/{binary_name}");
    if is_file(&binary_path) {
        return Ok(binary_path);
    }

    let asset_name = format!("{asset_stem}.zip");
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| {
            format!(
                "{LS_REPO} {} has no asset named {asset_name}",
                release.version
            )
        })?;

    zed::set_language_server_installation_status(
        language_server_id,
        &zed::LanguageServerInstallationStatus::Downloading,
    );
    zed::download_file(
        &asset.download_url,
        &version_dir,
        zed::DownloadedFileType::Zip,
    )
    .map_err(|error| format!("failed to download {asset_name}: {error}"))?;
    zed::make_file_executable(&binary_path)?;

    remove_stale_versions(&version_dir);

    Ok(binary_path)
}

/// A `lemminx-*` directory from an earlier download, if its binary is still there.
fn find_installed_binary(binary_name: &str) -> Option<String> {
    let entries = fs::read_dir(".").ok()?;

    for entry in entries.flatten() {
        let directory = entry.file_name();
        let directory = directory.to_string_lossy();
        if !directory.starts_with("lemminx-") {
            continue;
        }

        let candidate = format!("{directory}/{binary_name}");
        if is_file(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn asset_stem(platform: zed::Os, arch: zed::Architecture) -> Result<&'static str> {
    match (platform, arch) {
        (zed::Os::Mac, zed::Architecture::Aarch64) => Ok("lemminx-osx-aarch_64"),
        (zed::Os::Mac, zed::Architecture::X8664) => Ok("lemminx-osx-x86_64"),
        (zed::Os::Linux, zed::Architecture::Aarch64) => Ok("lemminx-linux-aarch_64"),
        (zed::Os::Linux, zed::Architecture::X8664) => Ok("lemminx-linux-x86_64"),
        (zed::Os::Windows, zed::Architecture::X8664 | zed::Architecture::Aarch64) => {
            Ok("lemminx-win32")
        }
        (_, zed::Architecture::X86) => Err("32-bit x86 is not supported by LemMinX".to_string()),
    }
}

/// Drop `lemminx-*` directories left by older releases.
fn remove_stale_versions(current_dir: &str) {
    let Ok(entries) = fs::read_dir(".") else {
        return;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("lemminx-") && name.as_ref() != current_dir {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn is_file(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}
