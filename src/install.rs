// roblox/studio-rust-mcp-server/studio-rust-mcp-server-3fa6f326b335050f8cdd331df35decad0c9ab575/src/install.rs

use color_eyre::eyre::{Result, WrapErr};
use roblox_install::RobloxStudio;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

fn get_message() -> String {
    format!(
"Roblox Studio Gemini Connector is installed!

Next Steps:
1. Get a Gemini API Key from Google AI Studio.
2. Set the environment variable 'GEMINI_API_KEY' to your key.
3. Run this application again with the '--serve' flag:
   cargo run -- --serve
4. Open Roblox Studio and enable the 'MCPStudioPlugin' in the Plugins tab.
5. Send prompts to the server using a tool like Postman or curl, for example:

   curl -X POST http://127.0.0.1:44755/prompt \\
     -H \"Content-Type: application/json\" \\
     -d '{{\"prompt\": \"insert a red car\"}}'

To uninstall, delete 'MCPStudioPlugin.rbxm' from your Roblox plugins directory."
    )
}

async fn install_internal() -> Result<String> {
    // Install the Roblox plugin
    let plugin_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/MCPStudioPlugin.rbxm"));
    let studio = RobloxStudio::locate()?;
    let plugins_path = studio.plugins_path();
    if let Err(err) = fs::create_dir_all(plugins_path) {
        if err.kind() != io::ErrorKind::AlreadyExists {
            return Err(err.into());
        }
    }

    let output_plugin_path = Path::new(plugins_path).join("MCPStudioPlugin.rbxm");
    let mut file = File::create(&output_plugin_path).wrap_err_with(|| {
        format!(
            "Could not write Roblox Plugin file at {}",
            output_plugin_path.display()
        )
    })?;
    file.write_all(plugin_bytes)?;

    println!(
        "Installed Roblox Studio plugin to {}",
        output_plugin_path.display()
    );
    println!();

    let msg = get_message();
    println!("{}", msg);
    Ok(msg)
}

// Platform-specific logic to show the final message
#[cfg(target_os = "windows")]
pub async fn install() -> Result<()> {
    use std::process::Command;
    if let Err(e) = install_internal().await {
        tracing::error!("Failed to install Roblox Gemini Connector: {:#}", e);
    }
    let _ = Command::new("cmd.exe").arg("/c").arg("pause").status();
    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn install() -> Result<()> {
    use native_dialog::{DialogBuilder, MessageLevel};
    let alert_builder = match install_internal().await {
        Err(e) => DialogBuilder::new()
            .set_level(MessageLevel::Error)
            .set_text(&format!("Errors occurred: {:#}", e)),
        Ok(msg) => DialogBuilder::new()
            .set_level(MessageLevel::Info)
            .set_text(&msg),
    };
    let _ = alert_builder.set_title("Roblox Studio Gemini Connector").show_alert();
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub async fn install() -> Result<()> {
    install_internal().await?;
    Ok(())
}