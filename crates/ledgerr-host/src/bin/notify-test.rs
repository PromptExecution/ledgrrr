use ledgerr_host::notify::{
    NativeToastNotifier, NotificationBackend, NotificationSettings, NotificationStatus, Notifier,
};
use ledgerr_host::settings::{default_settings_path, SettingsStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend_override = None;
    let mut title = String::from("l3dg3rr");
    let mut body = String::from("toast test");

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--backend" => {
                let value = args.next().unwrap_or_else(|| "auto".to_string());
                backend_override = Some(match value.as_str() {
                    "native" | "powershell" => NotificationBackend::Native,
                    "noop" => NotificationBackend::Noop,
                    _ => NotificationBackend::Auto,
                });
            }
            "--title" => {
                title = args.next().unwrap_or_else(|| title.clone());
            }
            "--body" => {
                body = args.next().unwrap_or_else(|| body.clone());
            }
            _ => {}
        }
    }

    let store = SettingsStore::new(default_settings_path());
    let mut settings = store.load()?;
    if let Some(backend) = backend_override {
        settings.toast_backend_preference = backend;
    }

    let output = match settings.toast_backend_preference {
        NotificationBackend::Noop => serde_json::json!({
            "backend": "noop",
            "status": "disabled",
            "message": "noop backend selected"
        }),
        NotificationBackend::Auto | NotificationBackend::Native => {
            let notify_settings = NotificationSettings {
                enabled: settings.toast_enabled,
                backend: settings.toast_backend_preference,
                last_test_result: settings.last_test_result.clone(),
            };
            let notifier = NativeToastNotifier::new(notify_settings);
            match notifier.test(&title, &body) {
                Ok(result) => {
                    settings.last_test_result = Some(result.clone());
                    store.save(&settings)?;
                    serde_json::json!({
                        "backend": "native",
                        "status": match result.status {
                            NotificationStatus::Disabled => "disabled",
                            NotificationStatus::Unknown => "unknown",
                            NotificationStatus::Ready => "ready",
                            NotificationStatus::Degraded => "degraded",
                            NotificationStatus::Failed => "failed",
                        },
                        "result": result,
                    })
                }
                Err(err) => {
                    serde_json::json!({
                        "backend": "native",
                        "status": "failed",
                        "error": err.to_string(),
                    })
                }
            }
        }
    };

    println!("{}", serde_json::to_string_pretty(&output)?);

    if output["status"] == "failed" {
        std::process::exit(1);
    }

    Ok(())
}
