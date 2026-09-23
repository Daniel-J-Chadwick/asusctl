use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use log::{debug, error, info};
use rog_aura::keyboard::LaptopAuraPower;
use rog_aura::{AuraDeviceType, PowerZones};
use rog_dbus::zbus_aura::AuraProxy;
use slint::{ComponentHandle, Model, RgbaColor, SharedString};

use crate::config::Config;
use crate::ui::show_toast;
use crate::{AuraPageData, MainWindow, PowerZones as SlintPowerZones};

fn selection_is_current(generation: &AtomicUsize, selected: usize) -> bool {
    generation.load(Ordering::SeqCst) == selected
}

fn decode_hex(s: &str) -> RgbaColor<u8> {
    let s = s.trim_start_matches('#');
    if s.len() < 6 {
        return RgbaColor {
            alpha: 255,
            red: 0,
            green: 0,
            blue: 0,
        };
    }
    let c: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(164))
        .collect();
    RgbaColor {
        alpha: 255,
        red: *c.first().unwrap_or(&255),
        green: *c.get(1).unwrap_or(&128),
        blue: *c.get(2).unwrap_or(&32),
    }
}

async fn aura_ifaces() -> Result<Vec<(SharedString, AuraProxy<'static>)>, Box<dyn std::error::Error>>
{
    let conn = zbus::Connection::system().await?;
    let mgr = zbus::fdo::ObjectManagerProxy::new(&conn, "xyz.ljones.Asusd", "/").await?;
    let objs = mgr.get_managed_objects().await?;
    let mut paths: Vec<zbus::zvariant::OwnedObjectPath> = objs
        .iter()
        .filter(|(_, ifaces)| ifaces.keys().any(|k| k.as_str() == "xyz.ljones.Aura"))
        .map(|(p, _)| p.clone())
        .collect();
    paths.sort_by_key(|path| path.to_string());
    let mut devices = Vec::new();
    for path in paths {
        let proxy = AuraProxy::builder(&conn)
            .path(path)?
            .destination("xyz.ljones.Asusd")?
            .build()
            .await?;
        let name: SharedString = match proxy.device_type().await? {
            AuraDeviceType::RearGlow => "Rear window".into(),
            AuraDeviceType::LaptopKeyboard2021
            | AuraDeviceType::LaptopKeyboardPre2021
            | AuraDeviceType::LaptopKeyboardTuf => "Keyboard".into(),
            _ => "Aura device".into(),
        };
        devices.push((name, proxy));
    }
    devices.sort_by_key(|(name, _)| if name.as_str() == "Keyboard" { 0 } else { 1 });
    if devices.is_empty() {
        return Err("No Aura interface".into());
    }
    Ok(devices)
}

pub async fn prefetch_supported_basic_modes() -> Option<Vec<i32>> {
    let proxy = aura_ifaces().await.ok()?.into_iter().next()?.1;
    let modes = proxy.supported_basic_modes().await.ok()?;
    Some(modes.iter().map(|n| (*n).into()).collect())
}

pub fn setup_aura_page(
    ui: &MainWindow,
    _states: Arc<Mutex<Config>>,
    prefetched_supported: Option<Vec<i32>>,
) {
    let g = ui.global::<AuraPageData>();
    g.on_cb_hex_from_colour(|c| {
        format!("#{:02X}{:02X}{:02X}", c.red(), c.green(), c.blue()).into()
    });
    g.on_cb_hex_to_colour(|s| decode_hex(s.as_str()).into());

    let handle = ui.as_weak();
    tokio::spawn(async move {
        let Ok(devices) = aura_ifaces().await else {
            info!("No aura interfaces");
            return;
        };
        let names: Vec<SharedString> = devices.iter().map(|(name, _)| name.clone()).collect();
        let rear = devices
            .iter()
            .find(|(name, _)| name.as_str() == "Rear window")
            .map(|(_, proxy)| proxy.clone());
        let initial_rear = (names[0].as_str() == "Keyboard")
            .then(|| rear.clone())
            .flatten();
        let proxies = Arc::new(
            devices
                .into_iter()
                .map(|(_, proxy)| proxy)
                .collect::<Vec<_>>(),
        );
        let generation = Arc::new(AtomicUsize::new(0));
        let select_proxies = proxies.clone();
        let select_generation = generation.clone();
        let select_handle = handle.clone();
        let select_rear = rear.clone();
        let select_names = names.clone();
        handle
            .upgrade_in_event_loop(move |h| {
                h.global::<AuraPageData>()
                    .set_device_names(names.as_slice().into());
                h.global::<AuraPageData>().on_select_device(move |index| {
                    let Some(aura) = select_proxies.get(index as usize).cloned() else {
                        return;
                    };
                    let selected = select_generation.fetch_add(1, Ordering::SeqCst) + 1;
                    let rear_to_reassert = (select_names[index as usize].as_str() == "Keyboard")
                        .then(|| select_rear.clone())
                        .flatten();
                    tokio::spawn(load_aura_page(
                        select_handle.clone(),
                        aura,
                        rear_to_reassert,
                        None,
                        select_generation.clone(),
                        selected,
                    ));
                });
            })
            .map_err(|e| error!("{e}"))
            .ok();
        load_aura_page(
            handle,
            proxies[0].clone(),
            initial_rear,
            prefetched_supported,
            generation,
            0,
        )
        .await;
    });
}

async fn load_aura_page(
    handle: slint::Weak<MainWindow>,
    aura: AuraProxy<'static>,
    rear_to_reassert: Option<AuraProxy<'static>>,
    prefetched_supported: Option<Vec<i32>>,
    generation: Arc<AtomicUsize>,
    selected: usize,
) {
    if !selection_is_current(&generation, selected) {
        return;
    }

    if let Ok(value) = aura.brightness().await {
        let current = generation.clone();
        handle
            .upgrade_in_event_loop(move |h| {
                if selection_is_current(&current, selected) {
                    h.global::<AuraPageData>().set_brightness(value.into());
                }
            })
            .ok();
    }
    if let Ok(value) = aura.led_power().await {
        let current = generation.clone();
        handle
            .upgrade_in_event_loop(move |h| {
                if selection_is_current(&current, selected) {
                    h.global::<AuraPageData>().set_led_power(value.into());
                }
            })
            .ok();
    }
    if let Ok(value) = aura.device_type().await {
        let current = generation.clone();
        handle
            .upgrade_in_event_loop(move |h| {
                if selection_is_current(&current, selected) {
                    h.global::<AuraPageData>().set_device_type(value.into());
                }
            })
            .ok();
    }
    if !selection_is_current(&generation, selected) {
        return;
    }

    let modes_vec: Vec<i32> = match prefetched_supported {
        Some(p) => p,
        None => aura
            .supported_basic_modes()
            .await
            .ok()
            .map(|m| m.iter().map(|n| (*n).into()).collect())
            .unwrap_or_default(),
    };

    // Restore saved mode, colours, zone, speed, direction from asusd (persisted to disk).
    // Use effect.mode as single source — avoid led_mode() which can fail (try_lock).
    let restore = aura.led_mode_data().await.ok();
    let raw_mode: Option<i32> = restore.as_ref().map(|d| d.mode.into());
    let d_slint = restore.map(|d| d.into());
    let current = generation.clone();
    handle
        .upgrade_in_event_loop(move |h| {
            if !selection_is_current(&current, selected) {
                return;
            }
            let names = h.global::<AuraPageData>().get_mode_names();
            let mut raws = Vec::new();
            let mut mode_names = Vec::new();
            for (i, name) in names.iter().enumerate() {
                let raw = i as i32;
                if modes_vec.contains(&raw) && i != 9 {
                    raws.push(raw);
                    mode_names.push(name.clone());
                }
            }
            h.global::<AuraPageData>()
                .set_supported_basic_modes(raws.as_slice().into());
            h.global::<AuraPageData>()
                .set_available_mode_names(mode_names.as_slice().into());
            if let Some(d) = d_slint {
                h.global::<AuraPageData>().invoke_update_led_mode_data(d);
                if let Some(cm) = raw_mode {
                    let idx = raws.iter().position(|&r| r == cm).unwrap_or(0) as i32;
                    h.global::<AuraPageData>().set_current_available_mode(idx);
                }
                h.invoke_external_colour_change();
            }
        })
        .map_err(|e| error!("{e}"))
        .ok();

    if let Ok(mut pow3r) = aura.supported_power_zones().await {
        let dev = aura
            .device_type()
            .await
            .unwrap_or(AuraDeviceType::LaptopKeyboard2021);
        let current = generation.clone();
        handle
            .upgrade_in_event_loop(move |handle| {
                if !selection_is_current(&current, selected) {
                    return;
                }
                let names: Vec<SharedString> = handle
                    .global::<AuraPageData>()
                    .get_power_zone_names()
                    .iter()
                    .collect();
                if dev.is_old_laptop() {
                    pow3r.retain(|z| *z != PowerZones::None);
                    let n: Vec<SharedString> =
                        pow3r.iter().map(|z| names[(*z) as usize].clone()).collect();
                    handle
                        .global::<AuraPageData>()
                        .set_power_zone_names_old(n.as_slice().into());
                } else {
                    let p: Vec<SlintPowerZones> = pow3r
                        .iter()
                        .filter(|z| **z != PowerZones::None)
                        .map(|z| (*z).into())
                        .collect();
                    handle
                        .global::<AuraPageData>()
                        .set_supported_power_zones(p.as_slice().into());
                }
            })
            .ok();
    }

    let proxy = aura.clone();
    let weak = handle.clone();
    let current = generation.clone();
    handle
        .upgrade_in_event_loop(move |h| {
            if !selection_is_current(&current, selected) {
                return;
            }
            let brightness_proxy = proxy.clone();
            let brightness_weak = weak.clone();
            let brightness_rear = rear_to_reassert.clone();
            let brightness_generation = current.clone();
            h.global::<AuraPageData>().on_cb_brightness(move |value| {
                if !selection_is_current(&brightness_generation, selected) {
                    return;
                }
                let p = brightness_proxy.clone();
                let w = brightness_weak.clone();
                let rear = brightness_rear.clone();
                tokio::spawn(async move {
                    let result = async {
                        p.set_brightness(value.into()).await?;
                        if let Some(rear) = rear {
                            let saved = rear.brightness().await?;
                            rear.set_brightness(saved).await?;
                        }
                        Ok(())
                    }
                    .await;
                    show_toast(
                        format!("Brightness set to {value}").into(),
                        "Brightness failed".into(),
                        w,
                        result,
                    );
                });
            });

            let p = proxy.clone();
            let w = weak.clone();
            let effect_generation = current.clone();
            h.global::<AuraPageData>().on_apply_led_mode_data(move || {
                if !selection_is_current(&effect_generation, selected) {
                    return;
                }
                let Some(ui) = w.upgrade() else { return };
                let slint_effect = ui.global::<AuraPageData>().get_led_mode_data();
                let raw: rog_aura::AuraEffect = slint_effect.into();
                let pp = p.clone();
                let t = w.clone();
                tokio::spawn(async move {
                    let r = pp.set_led_mode_data(raw).await;
                    show_toast("LED mode applied".into(), "LED mode failed".into(), t, r);
                });
            });
            h.invoke_external_colour_change();
        })
        .ok();

    let brightness_stream = aura.clone();
    let brightness_handle = handle.clone();
    let brightness_generation = generation.clone();
    tokio::spawn(async move {
        use futures_util::StreamExt;
        let mut stream = brightness_stream.receive_brightness_changed().await;
        while let Some(event) = stream.next().await {
            if !selection_is_current(&brightness_generation, selected) {
                break;
            }
            if let Ok(value) = event.get().await {
                let current = brightness_generation.clone();
                brightness_handle
                    .upgrade_in_event_loop(move |h| {
                        if selection_is_current(&current, selected) {
                            h.global::<AuraPageData>().set_brightness(value.into());
                        }
                    })
                    .ok();
            }
        }
    });

    let weak_power = handle.clone();
    let proxy_power = aura.clone();
    let current = generation.clone();
    handle
        .upgrade_in_event_loop(move |h| {
            if !selection_is_current(&current, selected) {
                return;
            }
            let power_generation = current.clone();
            h.global::<AuraPageData>().on_cb_led_power(move |power| {
                if !selection_is_current(&power_generation, selected) {
                    return;
                }
                let w = weak_power.clone();
                let p = proxy_power.clone();
                let pw: LaptopAuraPower = power.into();
                tokio::spawn(async move {
                    show_toast(
                        "Aura power updated".into(),
                        "Aura power failed".into(),
                        w,
                        p.set_led_power(pw).await,
                    );
                });
            });
        })
        .map_err(|e| error!("{e}"))
        .ok();

    let stream_handle = handle.clone();
    let stream_generation = generation.clone();
    let aura_stream = aura.clone();
    tokio::spawn(async move {
        use futures_util::StreamExt;
        let mut stream = aura_stream.receive_led_mode_data_changed().await;
        while let Some(e) = stream.next().await {
            if !selection_is_current(&stream_generation, selected) {
                break;
            }
            if let Ok(out) = e.get().await {
                let raw: i32 = out.mode.into();
                let data = out.into();
                let current = stream_generation.clone();
                stream_handle
                    .upgrade_in_event_loop(move |h| {
                        if !selection_is_current(&current, selected) {
                            return;
                        }
                        h.global::<AuraPageData>().invoke_update_led_mode_data(data);
                        let supported: Vec<i32> = h
                            .global::<AuraPageData>()
                            .get_supported_basic_modes()
                            .iter()
                            .collect();
                        let idx = supported.iter().position(|&x| x == raw).unwrap_or(0) as i32;
                        h.global::<AuraPageData>().set_current_available_mode(idx);
                        h.invoke_external_colour_change();
                    })
                    .map_err(|e| error!("{e}"))
                    .ok();
            }
        }
    });
    debug!("Aura setup done");
}
