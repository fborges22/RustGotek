//! Linux theme notifications supplement winit, which does not expose the desktop
//! color scheme on every backend. Other platforms use egui's native theme input.
use eframe::egui;

pub fn watch_system_theme(ctx: &egui::Context) {
    #[cfg(target_os = "linux")]
    {
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = watch_portal(&ctx);
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = ctx;
}

#[cfg(target_os = "linux")]
fn watch_portal(ctx: &egui::Context) -> zbus::Result<()> {
    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";
    let connection = zbus::blocking::connection::Builder::session()?
        .method_timeout(std::time::Duration::from_secs(2))
        .build()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )?;
    let signals = proxy.receive_signal_with_args("SettingChanged", &[(0, NAMESPACE), (1, KEY)])?;
    // Older portals wrap Read's result in an additional variant.
    let current: zbus::Result<zbus::zvariant::OwnedValue> = proxy
        .call("ReadOne", &(NAMESPACE, KEY))
        .or_else(|_| proxy.call("Read", &(NAMESPACE, KEY)));
    let update = |value: zbus::zvariant::OwnedValue| {
        if let Ok(scheme) = value.downcast_ref::<u32>() {
            ctx.options_mut(|options| options.fallback_theme = portal_theme(scheme));
            ctx.request_repaint();
        }
    };
    if let Ok(value) = current {
        update(value);
    }
    for signal in signals {
        if let Ok((namespace, key, value)) =
            signal
                .body()
                .deserialize::<(String, String, zbus::zvariant::OwnedValue)>()
            && namespace == NAMESPACE
            && key == KEY
        {
            update(value);
        }
    }
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn portal_theme(scheme: u32) -> egui::Theme {
    // Unknown and no-preference values use the application's light fallback.
    if scheme == 1 {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portal_values_and_explicit_override() {
        assert_eq!(portal_theme(1), egui::Theme::Dark);
        for value in [0, 2, 99] {
            assert_eq!(portal_theme(value), egui::Theme::Light);
        }
        let ctx = egui::Context::default();
        ctx.options_mut(|o| o.fallback_theme = portal_theme(1));
        ctx.set_theme(egui::ThemePreference::System);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        ctx.set_theme(egui::ThemePreference::Light);
        assert_eq!(ctx.theme(), egui::Theme::Light);
    }
}
